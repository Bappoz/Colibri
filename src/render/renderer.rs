//! The frame pipeline: turns a [`Scene`] into pixels.
//!
//! ```text
//!   model space          world          clip space        screen
//!  ┌───────────┐  model  ┌─────┐  view  ┌──────────┐ /w  ┌────────┐
//!  │ Mesh      │ ──────► │     │ ─────► │ clip +   │ ──► │ raster │
//!  │ vertices  │  matrix │ ... │  proj  │ cull     │     │ + depth│
//!  └───────────┘         └─────┘        └──────────┘     └────────┘
//! ```
//!
//! Vertices are transformed **once per object**, not once per triangle: a
//! vertex shared by six faces used to pay for six matrix products. The
//! per-object matrices are also pre-multiplied into a single MVP, which
//! removes a full 4x4 matrix product from every single vertex.

use crate::assets::Assets;
use crate::math::{Mat4x4, Vec3d};
use crate::render::clip::{ClipVertex, clip_near, trivial_reject};
use crate::render::raster::{ScreenVertex, draw_wireframe, fill_triangle, signed_area};
use crate::render::target::RenderTarget;
use crate::scene::{RenderObject, Scene};

/// Debug palette cycled per triangle when
/// [`RenderOptions::debug_triangle_tint`] is on. Six colors is enough to make
/// adjacent triangles of a cube face distinguishable.
const DEBUG_TINTS: [u32; 6] = [
    0x00FF0000, 0x0000FF00, 0x000000FF, 0x00FFFF00, 0x00FF00FF, 0x0000FFFF,
];

/// Color of the wireframe overlay.
const WIREFRAME_COLOR: u32 = 0x00FFFFFF;

/// Knobs that change how a frame is drawn, without touching the scene.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Background the frame is cleared to, `0x00RRGGBB`.
    pub clear_color: u32,
    /// Discard triangles facing away from the camera. Roughly halves the
    /// rasterization work on a closed mesh; turning it off is the quickest way
    /// to check whether a model has inconsistent winding.
    pub backface_culling: bool,
    /// Tint each triangle with a color from a fixed debug palette instead of the
    /// object's own tint, making the tessellation visible.
    pub debug_triangle_tint: bool,
    /// Draw triangle edges on top of the filled surface.
    pub wireframe: bool,
}

impl Default for RenderOptions {
    /// Black background, culling on, no debug overlay.
    fn default() -> Self {
        Self {
            clear_color: 0x0000_0000,
            backface_culling: true,
            debug_triangle_tint: false,
            wireframe: false,
        }
    }
}

/// Per-frame counters, useful both for the log and for judging an
/// optimization by something other than feel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Objects visited in the scene.
    pub objects: u64,
    /// Triangles read from the meshes.
    pub triangles_submitted: u64,
    /// Triangles thrown away by the frustum trivial reject, before clipping.
    pub triangles_rejected: u64,
    /// Triangles thrown away by back-face culling.
    pub triangles_culled: u64,
    /// Triangles that reached the rasterizer, including the extra ones the
    /// near-plane clip produced.
    pub triangles_drawn: u64,
    /// Fragments that passed the depth test and were written.
    pub pixels_shaded: u64,
}

/// Owns the depth buffer and the scratch memory the pipeline reuses between
/// frames.
///
/// Everything that would otherwise be allocated per frame lives here, so a
/// steady-state frame performs no heap allocation at all.
pub struct Renderer {
    /// Depth buffer, `width * height` entries.
    depth: Vec<f32>,
    /// Viewport width in pixels.
    width: usize,
    /// Viewport height in pixels.
    height: usize,
    /// Transformed vertices of the object currently being drawn.
    vertex_cache: Vec<ClipVertex>,
    /// Counters from the most recent frame.
    stats: RenderStats,
    /// Debug and culling switches.
    pub options: RenderOptions,
}

impl Renderer {
    /// Creates a renderer with a zero-sized viewport; call
    /// [`Renderer::resize`] before the first frame.
    pub fn new(options: RenderOptions) -> Self {
        Self {
            depth: Vec::new(),
            width: 0,
            height: 0,
            vertex_cache: Vec::new(),
            stats: RenderStats::default(),
            options,
        }
    }

    /// Resizes the viewport and reallocates the depth buffer.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.depth.clear();
        self.depth.resize(width * height, f32::INFINITY);
    }

    /// Viewport width in pixels.
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Viewport height in pixels.
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Viewport aspect ratio, or `1.0` while the viewport is degenerate.
    pub fn aspect_ratio(&self) -> f64 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f64 / self.height as f64
        }
    }

    /// Counters from the most recently rendered frame.
    pub const fn stats(&self) -> RenderStats {
        self.stats
    }

    /// Draws `scene` into `color`, which must hold at least
    /// `width * height` pixels.
    ///
    /// Returns the counters for this frame; they are also kept in
    /// [`Renderer::stats`].
    pub fn render(&mut self, color: &mut [u32], scene: &Scene, assets: &Assets) -> RenderStats {
        let mut stats = RenderStats::default();
        if self.width == 0 || self.height == 0 {
            self.stats = stats;
            return stats; // minimized window: nothing to draw into
        }

        let options = self.options;
        // Moved out so the scratch buffer and the depth buffer can be borrowed
        // at the same time; put back before returning, keeping its capacity.
        let mut cache = std::mem::take(&mut self.vertex_cache);
        let mut target = RenderTarget::new(color, &mut self.depth, self.width, self.height);
        target.clear(options.clear_color);

        let aspect = if self.height == 0 {
            1.0
        } else {
            self.width as f64 / self.height as f64
        };
        // One product for the whole frame instead of one per vertex.
        let view_projection = scene.camera.projection_matrix(aspect) * scene.camera.view_matrix();

        for object in scene.objects() {
            stats.objects += 1;
            draw_object(
                &mut target,
                &mut cache,
                object,
                scene,
                assets,
                &view_projection,
                &options,
                &mut stats,
            );
        }

        self.vertex_cache = cache;
        self.stats = stats;
        stats
    }
}

/// Draws a single object: transform every vertex once, then walk its index
/// buffer.
#[allow(clippy::too_many_arguments)] // the alternative is a struct that exists
// only to be destructured immediately; every argument here is a distinct
// borrow the borrow checker needs to see separately.
fn draw_object(
    target: &mut RenderTarget<'_>,
    cache: &mut Vec<ClipVertex>,
    object: &RenderObject,
    scene: &Scene,
    assets: &Assets,
    view_projection: &Mat4x4,
    options: &RenderOptions,
    stats: &mut RenderStats,
) {
    let mesh = assets.mesh(object.mesh);
    let texture = assets.texture(object.texture);

    let model = object.transform.matrix();
    let model_view_projection = *view_projection * model;

    // --- vertex stage: once per unique vertex, not once per triangle ---
    cache.clear();
    cache.reserve(mesh.vertices.len());
    for vertex in &mesh.vertices {
        let position = Vec3d::new(
            vertex.position[0] as f64,
            vertex.position[1] as f64,
            vertex.position[2] as f64,
        );
        let normal = Vec3d::new(
            vertex.normal[0] as f64,
            vertex.normal[1] as f64,
            vertex.normal[2] as f64,
        );

        // Rotating the normal by the model matrix is only exact for a uniform
        // scale; see `Transform::matrix`. The re-normalize absorbs the scale.
        let world_normal = model.transform_direction(normal).normalize();

        cache.push(ClipVertex {
            pos: model_view_projection.transform_point(position),
            uv: [vertex.uv[0] as f64, vertex.uv[1] as f64],
            intensity: scene.light.intensity_for(world_normal),
        });
    }

    // --- triangle stage ---
    let (width, height) = (target.width() as f64, target.height() as f64);

    for (index, face) in mesh.indices.chunks_exact(3).enumerate() {
        stats.triangles_submitted += 1;

        let triangle = [
            cache[face[0] as usize],
            cache[face[1] as usize],
            cache[face[2] as usize],
        ];

        if trivial_reject(&triangle[0], &triangle[1], &triangle[2]) {
            stats.triangles_rejected += 1;
            continue;
        }

        let tint = if options.debug_triangle_tint {
            DEBUG_TINTS[index % DEBUG_TINTS.len()]
        } else {
            object.tint
        };

        // Clipping can split one triangle into two; both are drawn.
        for clipped in clip_near(triangle).as_slice() {
            let screen: [ScreenVertex; 3] =
                std::array::from_fn(|i| ScreenVertex::from_clip(&clipped[i], width, height));

            // Front faces come out with a negative area after the y-flip of
            // the viewport transform — see the `raster` module docs.
            if options.backface_culling && signed_area(&screen) >= 0.0 {
                stats.triangles_culled += 1;
                continue;
            }

            stats.triangles_drawn += 1;
            stats.pixels_shaded += fill_triangle(target, screen, texture, tint);

            if options.wireframe {
                draw_wireframe(target, &screen, WIREFRAME_COLOR);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{Mesh, Texture};
    use crate::math::Vec3d;
    use crate::scene::{Transform, camera::Camera};

    /// Cena com um quad texturizado a 3 unidades da câmera, olhando para ela.
    fn quad_scene() -> (Scene, Assets) {
        let mut assets = Assets::new();
        let mesh = assets.add_mesh(Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let mut scene = Scene::new();
        scene.camera = Camera::new(Vec3d::ZERO);
        scene.light.ambient = 1.0;
        scene.spawn(RenderObject::new(
            Transform::from_translation(Vec3d::new(0.0, 0.0, -3.0)),
            mesh,
            texture,
        ));
        (scene, assets)
    }

    /// Um quad na frente da câmera precisa acender pixels.
    #[test]
    fn renders_a_visible_object() {
        let (scene, assets) = quad_scene();
        let mut renderer = Renderer::new(RenderOptions::default());
        renderer.resize(64, 64);
        let mut color = vec![0_u32; 64 * 64];

        let stats = renderer.render(&mut color, &scene, &assets);

        assert_eq!(stats.objects, 1);
        assert_eq!(stats.triangles_submitted, 2);
        assert!(stats.pixels_shaded > 0, "nada foi desenhado");
        assert_eq!(color[64 * 32 + 32], 0x00FFFFFF, "centro da tela");
    }

    /// O mesmo quad atrás da câmera não pode aparecer.
    #[test]
    fn objects_behind_the_camera_are_discarded() {
        let (mut scene, assets) = quad_scene();
        for (entity, _) in scene.iter().map(|(e, o)| (e, *o)).collect::<Vec<_>>() {
            scene.get_mut(entity).unwrap().transform.translation = Vec3d::new(0.0, 0.0, 3.0);
        }

        let mut renderer = Renderer::new(RenderOptions::default());
        renderer.resize(64, 64);
        let mut color = vec![0_u32; 64 * 64];

        let stats = renderer.render(&mut color, &scene, &assets);

        assert_eq!(stats.pixels_shaded, 0);
        assert!(color.iter().all(|&c| c == 0));
    }

    /// Sem back-face culling o mesmo quad, virado, volta a ser desenhado.
    #[test]
    fn culling_can_be_disabled() {
        let (mut scene, assets) = quad_scene();
        let entity = scene.iter().map(|(e, _)| e).next().unwrap();
        scene.get_mut(entity).unwrap().transform.rotation =
            Vec3d::new(0.0, std::f64::consts::PI, 0.0);

        let mut renderer = Renderer::new(RenderOptions::default());
        renderer.resize(64, 64);
        let mut color = vec![0_u32; 64 * 64];

        let culled = renderer.render(&mut color, &scene, &assets);
        assert_eq!(culled.pixels_shaded, 0);

        renderer.options.backface_culling = false;
        let unculled = renderer.render(&mut color, &scene, &assets);
        assert!(unculled.pixels_shaded > 0);
    }

    /// Um frame numa janela minimizada não pode quebrar nem desenhar.
    #[test]
    fn zero_sized_viewport_is_a_no_op() {
        let (scene, assets) = quad_scene();
        let mut renderer = Renderer::new(RenderOptions::default());
        renderer.resize(0, 0);

        let stats = renderer.render(&mut [], &scene, &assets);
        assert_eq!(stats, RenderStats::default());
    }

    /// Cena vazia limpa a tela e reporta zero em tudo.
    #[test]
    fn empty_scene_clears_the_frame() {
        let mut renderer = Renderer::new(RenderOptions {
            clear_color: 0x00123456,
            ..RenderOptions::default()
        });
        renderer.resize(8, 8);
        let mut color = vec![0xFFFFFF_u32; 64];

        let stats = renderer.render(&mut color, &Scene::new(), &Assets::new());

        assert_eq!(stats.triangles_submitted, 0);
        assert!(color.iter().all(|&c| c == 0x00123456));
    }
}
