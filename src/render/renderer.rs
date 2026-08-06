//! The frame pipeline: turns a [`Scene`] into pixels.
//!
//! ```text
//!   model space          world          clip space         screen
//!  ┌───────────┐  model  ┌─────┐  view  ┌──────────┐  /w  ┌──────────┐
//!  │ Mesh      │ ──────► │     │ ─────► │ clip +   │ ───► │ raster   │
//!  │ vertices  │  matrix │ ... │  proj  │ cull     │      │ + depth  │
//!  └───────────┘         └─────┘        └──────────┘      └──────────┘
//!        └──────── geometry stage ───────────┘   └── raster stage ──┘
//!                   (single threaded)              (one thread per band)
//! ```
//!
//! # Two stages, split on purpose
//!
//! The geometry stage transforms vertices **once per object** — a vertex
//! shared by six faces used to pay for six matrix products — and writes every
//! surviving triangle into a flat list of raster jobs in screen space. The
//! per-object matrices are also pre-multiplied into a single MVP, which
//! removes a full 4x4 matrix product from every vertex.
//!
//! The raster stage then splits the frame into horizontal **bands** and hands
//! them to worker threads. Bands own disjoint slices of the color and depth
//! buffers, so no pixel is ever written by two threads and no synchronization
//! is needed inside the inner loop. The output is byte-for-byte identical to
//! the single-threaded path, whatever the thread count — a band always sees
//! the same jobs in the same order.

use std::sync::Mutex;

use crate::assets::{Assets, TextureHandle};
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

/// How many bands each worker thread gets on average.
///
/// More bands than threads is what keeps the workers busy when the geometry
/// is not spread evenly down the screen: with one band per thread, a scene
/// centered on the horizon leaves the top and bottom workers idle. Too many
/// and the per-band overhead — re-testing every job against the band — starts
/// to show.
const BANDS_PER_THREAD: usize = 4;

/// Minimum band height in rows, so a short viewport does not end up with more
/// bands than it has rows of pixels.
const MIN_BAND_ROWS: usize = 16;

/// Knobs that change how a frame is drawn, without touching the scene.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// Background the frame is cleared to, `0x00RRGGBB`.
    pub clear_color: u32,
    /// Discard triangles facing away from the camera. Roughly halves the
    /// rasterization work on a closed mesh; turning it off is the quickest way
    /// to check whether a model has inconsistent winding.
    pub backface_culling: bool,
    /// Tint each triangle with a color from a fixed debug palette instead of
    /// the object's own tint, making the tessellation visible.
    pub debug_triangle_tint: bool,
    /// Draw triangle edges on top of the filled surface.
    pub wireframe: bool,
    /// Worker threads for the raster stage. `0` asks the platform how many
    /// cores are available; `1` keeps everything on the calling thread.
    ///
    /// Rasterization is the part of the frame that scales: the threads write
    /// to disjoint bands of the framebuffer, so the result does not depend on
    /// this value.
    pub threads: usize,
}

impl Default for RenderOptions {
    /// Black background, culling on, no debug overlay, one worker per core.
    fn default() -> Self {
        Self {
            clear_color: 0x0000_0000,
            backface_culling: true,
            debug_triangle_tint: false,
            wireframe: false,
            threads: 0,
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

impl RenderStats {
    /// Folds the counters of one worker into the frame total.
    fn merge(&mut self, other: RenderStats) {
        self.objects += other.objects;
        self.triangles_submitted += other.triangles_submitted;
        self.triangles_rejected += other.triangles_rejected;
        self.triangles_culled += other.triangles_culled;
        self.triangles_drawn += other.triangles_drawn;
        self.pixels_shaded += other.pixels_shaded;
    }
}

/// A triangle that survived clipping and culling: screen space, ready to fill.
///
/// The geometry stage produces these and the raster stage consumes them, which
/// is what lets the two run under different threading rules.
#[derive(Clone, Copy)]
struct RasterJob {
    /// The three vertices in framebuffer coordinates.
    vertices: [ScreenVertex; 3],
    /// Texture to sample across the triangle.
    texture: TextureHandle,
    /// Color modulated into every sampled texel.
    tint: u32,
    /// Topmost row the triangle can touch, in framebuffer coordinates.
    min_y: f64,
    /// Bottommost row the triangle can touch.
    max_y: f64,
}

impl RasterJob {
    /// Whether the triangle can touch the rows `[y0, y0 + rows)`.
    ///
    /// A cheap reject: with a dozen bands, most triangles belong to one or two
    /// of them and the rest of the workers skip them without setting up a
    /// rasterization.
    #[inline]
    fn touches(&self, y0: usize, rows: usize) -> bool {
        self.max_y >= y0 as f64 && self.min_y < (y0 + rows) as f64
    }
}

/// One horizontal slice of the frame, owned outright by whichever worker
/// claims it.
///
/// Bands never overlap, so the workers need no locks and no atomics on the
/// pixels themselves — the borrow checker proves the split is disjoint.
struct Band<'a> {
    /// First row of the band, in framebuffer coordinates.
    y0: usize,
    /// Number of rows in the band.
    rows: usize,
    /// The band's slice of the color buffer.
    color: &'a mut [u32],
    /// The band's slice of the depth buffer.
    depth: &'a mut [f32],
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
    /// Transformed vertices of the object currently being processed.
    vertex_cache: Vec<ClipVertex>,
    /// Triangles handed from the geometry stage to the raster stage.
    jobs: Vec<RasterJob>,
    /// Worker count resolved from [`RenderOptions::threads`], never zero.
    threads: usize,
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
            jobs: Vec::new(),
            threads: resolve_threads(options.threads),
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

    /// Worker threads currently used by the raster stage.
    ///
    /// Resolved from [`RenderOptions::threads`] when the renderer is built and
    /// re-resolved by [`Renderer::set_threads`]; never zero.
    pub const fn threads(&self) -> usize {
        self.threads
    }

    /// Changes the worker count. `0` asks the platform for the core count.
    pub fn set_threads(&mut self, threads: usize) {
        self.options.threads = threads;
        self.threads = resolve_threads(threads);
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
        let (width, height) = (self.width, self.height);

        // --- geometry stage ---------------------------------------------
        // Moved out so the scratch buffers and the depth buffer can be
        // borrowed at the same time; put back before returning, capacity
        // intact, so a steady-state frame allocates nothing.
        let mut jobs = std::mem::take(&mut self.jobs);
        let mut cache = std::mem::take(&mut self.vertex_cache);
        jobs.clear();

        // One product for the whole frame instead of one per vertex.
        let view_projection =
            scene.camera.projection_matrix(self.aspect_ratio()) * scene.camera.view_matrix();

        for object in scene.objects() {
            stats.objects += 1;
            submit_object(
                &mut jobs,
                &mut cache,
                object,
                scene,
                assets,
                &view_projection,
                &options,
                &mut stats,
                (width as f64, height as f64),
            );
        }
        stats.triangles_drawn = jobs.len() as u64;

        // --- raster stage -----------------------------------------------
        let bands = split_into_bands(color, &mut self.depth, width, height, self.threads);
        stats.merge(rasterize(bands, &jobs, assets, &options, self.threads));

        self.vertex_cache = cache;
        self.jobs = jobs;
        self.stats = stats;
        stats
    }
}

/// Turns [`RenderOptions::threads`] into a concrete worker count.
///
/// `0` means "ask the platform"; when it cannot answer — a container with no
/// visible CPU topology — one thread is the safe answer.
fn resolve_threads(requested: usize) -> usize {
    if requested > 0 {
        return requested;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

/// Cuts the color and depth buffers into disjoint horizontal bands.
///
/// The split is done with `chunks_mut`, so the borrow checker itself
/// guarantees the workers cannot touch each other's pixels.
fn split_into_bands<'a>(
    color: &'a mut [u32],
    depth: &'a mut [f32],
    width: usize,
    height: usize,
    threads: usize,
) -> Vec<Band<'a>> {
    let target_bands = (threads * BANDS_PER_THREAD).max(1);
    let rows = height.div_ceil(target_bands).max(MIN_BAND_ROWS);
    let pixels_per_band = rows * width;

    color[..width * height]
        .chunks_mut(pixels_per_band)
        .zip(depth[..width * height].chunks_mut(pixels_per_band))
        .enumerate()
        .map(|(index, (color, depth))| Band {
            y0: index * rows,
            // The last band is short whenever the height is not a multiple.
            rows: color.len() / width,
            color,
            depth,
        })
        .collect()
}

/// Clears and fills every band, on one thread or on many.
///
/// Below two threads the work queue and the scope are pure overhead, so the
/// serial path is not just a fallback — it is what a small viewport should
/// take.
fn rasterize(
    bands: Vec<Band<'_>>,
    jobs: &[RasterJob],
    assets: &Assets,
    options: &RenderOptions,
    threads: usize,
) -> RenderStats {
    if threads <= 1 || bands.len() <= 1 {
        let mut stats = RenderStats::default();
        for band in bands {
            stats.merge(draw_band(band, jobs, assets, options));
        }
        return stats;
    }

    // Bands are claimed one at a time instead of dealt out up front: a scene
    // sitting in the middle of the screen would otherwise leave the workers
    // holding the top and bottom bands with nothing to do.
    let queue = Mutex::new(bands);
    let mut stats = RenderStats::default();

    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut local = RenderStats::default();
                    loop {
                        // The guard is scoped to this block on purpose: in a
                        // `while let`, a temporary in the scrutinee lives until
                        // the end of the loop *body*, so the lock would be held
                        // across `draw_band` and every worker would serialize
                        // behind it. Measured parallelism in that shape was
                        // exactly 1.00x.
                        //
                        // A worker only panics on a bug, and a poisoned queue
                        // would bury it under a second panic — so the lock is
                        // taken back either way and the original panic wins.
                        let claimed = {
                            let mut queue = queue.lock().unwrap_or_else(|e| e.into_inner());
                            queue.pop()
                        };
                        let Some(band) = claimed else { break };
                        local.merge(draw_band(band, jobs, assets, options));
                    }
                    local
                })
            })
            .collect();

        for worker in workers {
            match worker.join() {
                Ok(local) => stats.merge(local),
                Err(payload) => std::panic::resume_unwind(payload),
            }
        }
    });

    stats
}

/// Clears one band and rasterizes every job that reaches it.
///
/// Clearing happens here rather than up front so it rides along with the
/// parallelism: at 1080p the two buffers are 16 MB per frame, which is not
/// free on one thread.
fn draw_band(
    band: Band<'_>,
    jobs: &[RasterJob],
    assets: &Assets,
    options: &RenderOptions,
) -> RenderStats {
    let Band {
        y0,
        rows,
        color,
        depth,
    } = band;
    let width = color.len() / rows.max(1);
    let mut target = RenderTarget::new(color, depth, width, rows);
    target.clear(options.clear_color);

    let mut stats = RenderStats::default();
    for job in jobs {
        if !job.touches(y0, rows) {
            continue;
        }

        // The band's rasterizer works in band-local coordinates, so the
        // triangle is shifted up by the band's origin. Three subtractions,
        // against a bounding box that then clamps to the band for free.
        let mut vertices = job.vertices;
        for vertex in &mut vertices {
            vertex.y -= y0 as f64;
        }

        stats.pixels_shaded +=
            fill_triangle(&mut target, vertices, assets.texture(job.texture), job.tint);

        if options.wireframe {
            draw_wireframe(&mut target, &vertices, WIREFRAME_COLOR);
        }
    }
    stats
}

/// Geometry stage for a single object: transform every vertex once, clip and
/// cull, and append what survives to `jobs`.
#[allow(clippy::too_many_arguments)] // the alternative is a struct that exists
// only to be destructured immediately; every argument here is a distinct
// borrow the borrow checker needs to see separately.
fn submit_object(
    jobs: &mut Vec<RasterJob>,
    cache: &mut Vec<ClipVertex>,
    object: &RenderObject,
    scene: &Scene,
    assets: &Assets,
    view_projection: &Mat4x4,
    options: &RenderOptions,
    stats: &mut RenderStats,
    viewport: (f64, f64),
) {
    let mesh = assets.mesh(object.mesh);
    let model = object.transform.matrix();
    let model_view_projection = *view_projection * model;
    let (width, height) = viewport;

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

        // Clipping can split one triangle into two; both are submitted.
        for clipped in clip_near(triangle).as_slice() {
            let vertices: [ScreenVertex; 3] =
                std::array::from_fn(|i| ScreenVertex::from_clip(&clipped[i], width, height));

            // Front faces come out with a negative area after the y-flip of
            // the viewport transform — see the `raster` module docs.
            if options.backface_culling && signed_area(&vertices) >= 0.0 {
                stats.triangles_culled += 1;
                continue;
            }

            let min_y = vertices.iter().map(|v| v.y).fold(f64::INFINITY, f64::min);
            let max_y = vertices
                .iter()
                .map(|v| v.y)
                .fold(f64::NEG_INFINITY, f64::max);

            jobs.push(RasterJob {
                vertices,
                texture: object.texture,
                tint,
                min_y,
                max_y,
            });
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
        let texture = assets.add_texture(Texture::checkerboard(64, 8));

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

    /// Renderiza a cena num buffer novo com a contagem de threads pedida.
    fn render_with(threads: usize, scene: &Scene, assets: &Assets) -> (Vec<u32>, RenderStats) {
        let mut renderer = Renderer::new(RenderOptions {
            threads,
            ..RenderOptions::default()
        });
        renderer.resize(129, 97); // dimensões primas: exercitam a banda curta
        let mut color = vec![0u32; 129 * 97];
        let stats = renderer.render(&mut color, scene, assets);
        (color, stats)
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
        assert_ne!(color[64 * 32 + 32], 0, "centro da tela");
    }

    /// O paralelismo não pode mudar o resultado: as bandas são disjuntas e
    /// cada uma vê os mesmos triângulos na mesma ordem.
    #[test]
    fn thread_count_does_not_change_the_frame() {
        let (scene, assets) = quad_scene();
        let (reference, reference_stats) = render_with(1, &scene, &assets);

        for threads in [2, 3, 8, 16] {
            let (frame, stats) = render_with(threads, &scene, &assets);
            assert_eq!(frame, reference, "frame diferente com {threads} threads");
            assert_eq!(stats, reference_stats, "stats diferentes com {threads}");
        }
    }

    /// Toda banda é limpa, inclusive as que nenhum triângulo alcança.
    #[test]
    fn every_band_is_cleared() {
        let mut renderer = Renderer::new(RenderOptions {
            clear_color: 0x0012_3456,
            threads: 4,
            ..RenderOptions::default()
        });
        renderer.resize(129, 97);
        let mut color = vec![0xFFFF_FFFF_u32; 129 * 97];

        renderer.render(&mut color, &Scene::new(), &Assets::new());

        assert!(color.iter().all(|&c| c == 0x0012_3456));
    }

    /// O mesmo quad atrás da câmera não pode aparecer.
    #[test]
    fn objects_behind_the_camera_are_discarded() {
        let (mut scene, assets) = quad_scene();
        let entity = scene.iter().map(|(e, _)| e).next().unwrap();
        scene.get_mut(entity).unwrap().transform.translation = Vec3d::new(0.0, 0.0, 3.0);

        let (color, stats) = render_with(4, &scene, &assets);

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

    /// `threads: 0` resolve para o paralelismo da máquina, nunca zero.
    #[test]
    fn thread_count_is_resolved_once() {
        let renderer = Renderer::new(RenderOptions::default());
        assert!(renderer.threads() >= 1);

        let mut renderer = renderer;
        renderer.set_threads(3);
        assert_eq!(renderer.threads(), 3);
    }
}
