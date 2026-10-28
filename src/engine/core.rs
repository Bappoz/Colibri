//! The engine: owns the window, the surface, and the update/render loop.
//!
//! Everything the frame actually needs lives elsewhere — the scene in
//! [`crate::scene`], the pixels in [`crate::render`], the data in
//! [`crate::assets`]. What is left here is the platform plumbing and the order
//! the pieces run in.

use std::num::NonZeroU32;
use std::path::Path;
use std::rc::Rc;

use softbuffer::{Context, Surface};
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

use crate::assets::{Assets, MeshHandle, Texture, TextureHandle, spawn_gltf_scene};
use crate::engine::clock::FrameClock;
use crate::engine::config::EngineConfig;
use crate::engine::input::InputState;
use crate::error::{Error, Result};
use crate::math::Vec3d;
use crate::render::Renderer;
use crate::scene::{Camera, MeshRenderer, Scene, Spin, Transform};

/// Size of the procedural checkerboard used when no texture file is given.
const CHECKER_SIZE: u32 = 256;
/// Size of one checkerboard cell, in texels.
const CHECKER_CELL: u32 = 32;

/// Radius of the ring of satellite objects, in normalized units.
const RING_RADIUS: f64 = 2.5;
/// Scale applied to the satellites, relative to the central object.
const RING_SCALE: f64 = 0.45;
/// Distance from the origin the demo camera starts at.
const CAMERA_DISTANCE: f64 = 5.0;
/// Height the demo camera starts at, so the ring is seen slightly from above.
const CAMERA_HEIGHT: f64 = 1.2;
/// Downward tilt of the demo camera, in radians.
const CAMERA_PITCH: f64 = -0.2;

/// Owns the window and drives one frame at a time.
pub struct Engine {
    /// The OS window. Reference counted because `softbuffer` needs to hold it
    /// as both the display handle and the window handle.
    window: Rc<Window>,
    /// The mapped framebuffer the renderer writes into.
    surface: Surface<Rc<Window>, Rc<Window>>,
    /// Depth buffer, scratch memory and rasterizer switches.
    renderer: Renderer,
    /// What there is to draw.
    scene: Scene,
    /// Camera pose restored by the `R` control, including imported-scene fit.
    camera_reset: Camera,
    /// Imported scene bounds used to adapt the initial fit to viewport resizes.
    auto_frame_bounds: Option<(Vec3d, f64)>,
    /// Resizing stops reframing once the user changes the camera.
    camera_manually_adjusted: bool,
    /// Meshes and textures, shared by handle across the scene.
    assets: Assets,
    /// Keyboard and mouse state accumulated between frames.
    input: InputState,
    /// Frame timing and the throttled performance report.
    clock: FrameClock,
    /// Startup configuration; the render switches inside it are the ones the
    /// runtime toggles write to.
    config: EngineConfig,
}

impl Engine {
    /// Creates the engine for an existing window, loading the assets named by
    /// `config`.
    ///
    /// Fails when an asset cannot be read, which is the only expected failure:
    /// after this returns, rendering a frame cannot fail.
    pub fn new(window: Window, config: EngineConfig) -> Result<Self> {
        let window = Rc::new(window);
        let initial_size = window.inner_size();
        grab_cursor(&window);

        let context = Context::new(window.clone())
            .map_err(|e| Error::Surface(format!("softbuffer context: {e}")))?;
        let surface = Surface::new(&context, window.clone())
            .map_err(|e| Error::Surface(format!("softbuffer surface: {e}")))?;

        let mut assets = Assets::new();
        let texture = match config.texture_path.as_deref() {
            Some(path) => assets.load_texture(path)?,
            None => assets.add_texture(Texture::checkerboard(CHECKER_SIZE, CHECKER_CELL)),
        };

        let extension = Path::new(&config.model_path)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| Error::UnsupportedModelFormat {
                path: config.model_path.clone(),
            })?;

        let (mut scene, obj_mesh, auto_frame) = match extension.as_str() {
            "obj" => {
                let mesh = assets.load_mesh(&config.model_path)?;
                let scene = build_demo_scene(mesh, texture, assets.mesh(mesh).bounding_radius());
                (scene, Some(mesh), false)
            }
            "gltf" | "glb" => {
                let mut scene = Scene::new();
                *scene.camera_mut() = default_camera();
                spawn_gltf_scene(&mut scene, &mut assets, &config.model_path, texture)?;
                (scene, None, true)
            }
            _ => {
                return Err(Error::UnsupportedModelFormat {
                    path: config.model_path.clone(),
                });
            }
        };

        let mut auto_frame_bounds = None;
        if auto_frame {
            let aspect = if initial_size.height > 0 {
                initial_size.width as f64 / initial_size.height as f64
            } else {
                1.0
            };
            match scene.bounding_sphere(&assets) {
                Some((center, radius))
                    if scene.camera_mut().frame_sphere(center, radius, aspect) =>
                {
                    auto_frame_bounds = Some((center, radius));
                    println!(
                        "[engine] framed imported scene (radius {radius:.3}, aspect {aspect:.3})"
                    );
                }
                _ => println!(
                    "[engine] scene bounds are empty or degenerate; using the default camera"
                ),
            }
        }
        let camera_reset = *scene.camera();

        println!(
            "[engine] loaded '{}': {} meshes, {} triangles",
            config.model_path,
            assets.mesh_count(),
            assets.total_triangles()
        );
        if obj_mesh.is_some_and(|mesh| !assets.mesh(mesh).has_texture_coords()) {
            println!(
                "[engine] note: '{}' has no UV data — the texture samples a single texel, \
                 so the surface shows only the lighting",
                config.model_path
            );
        }
        println!("[engine] scene: {} objects", scene.drawable_count());
        println!("[engine] H prints the controls; F/C/T toggle the debug views");

        let renderer = Renderer::new(config.render);
        println!("[engine] raster threads: {}", renderer.threads());

        let mut engine = Self {
            window,
            surface,
            renderer,
            scene,
            camera_reset,
            auto_frame_bounds,
            camera_manually_adjusted: false,
            assets,
            input: InputState::new(),
            clock: FrameClock::new(),
            config,
        };

        // Without this the surface keeps a zero size and the first frame never
        // reaches the screen.
        let size = engine.window.inner_size();
        engine.resize(size);
        Ok(engine)
    }

    /// The window the engine draws into.
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// The scene being rendered, for inspection or scripted changes.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// The scene being rendered, mutably.
    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }

    /// Routes a window event that is not handled by the application shell.
    pub fn handle_window_event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                match event.state {
                    ElementState::Pressed => {
                        self.input.key_down(code);
                        // `repeat` filters the OS auto-repeat, so holding a
                        // toggle key does not flicker the setting.
                        if !event.repeat {
                            self.handle_toggle(code);
                        }
                    }
                    ElementState::Released => self.input.key_up(code),
                }
            }
            // The key-up for anything held now goes to another window.
            WindowEvent::Focused(false) => self.input.release_all(),
            _ => {}
        }
    }

    /// Feeds raw mouse movement to the camera's input state.
    pub fn mouse_moved(&mut self, delta: (f64, f64)) {
        self.input.accumulate_mouse(delta.0, delta.1);
    }

    /// Resizes the surface, the depth buffer and the projection.
    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return; // minimized (0x0): nothing to resize to
        };

        if let Err(e) = self.surface.resize(width, height) {
            eprintln!("[engine] surface resize failed: {e}");
            return;
        }
        self.renderer
            .resize(size.width as usize, size.height as usize);

        // Public `scene_mut` access is a manual camera control too. Detect a
        // changed pose before applying any automatic adjustment on resize.
        let camera = self.scene.camera();
        if camera.position != self.camera_reset.position
            || camera.yaw != self.camera_reset.yaw
            || camera.pitch != self.camera_reset.pitch
            || camera.fov_y != self.camera_reset.fov_y
            || camera.near != self.camera_reset.near
            || camera.far != self.camera_reset.far
        {
            self.camera_manually_adjusted = true;
        }
        if let Some((center, radius)) = self.auto_frame_bounds
            && !self.camera_manually_adjusted
            && self
                .scene
                .camera_mut()
                .frame_sphere(center, radius, self.renderer.aspect_ratio())
        {
            self.camera_reset = *self.scene.camera();
        }

        println!(
            "[engine] resize -> {}x{} (aspect {:.3})",
            size.width,
            size.height,
            self.renderer.aspect_ratio()
        );
    }

    /// Advances the simulation by one frame.
    ///
    /// Runs before [`Engine::render`] and is the only place `dt` is consumed:
    /// input, camera, then the scene's own systems.
    pub fn update(&mut self) {
        let dt = self.clock.tick();
        let previous_camera = *self.scene.camera();

        let (dx, dy) = self.input.take_mouse_delta();
        self.scene
            .camera_mut()
            .process_mouse(dx, dy, self.config.mouse_sensitivity);
        self.scene
            .camera_mut()
            .process_keyboard(&self.input, self.config.move_speed, dt);

        let camera = self.scene.camera();
        if camera.position != previous_camera.position
            || camera.yaw != previous_camera.yaw
            || camera.pitch != previous_camera.pitch
        {
            self.camera_manually_adjusted = true;
        }

        self.scene.update(dt);

        if let Some(report) = self.clock.take_report() {
            let stats = self.renderer.stats();
            println!(
                "[frame {}] {:.1} fps | {:.2} ms | {} tris drawn, {} culled, {} rejected | {} px",
                report.frame,
                report.fps,
                report.frame_time_ms,
                stats.triangles_drawn,
                stats.triangles_culled,
                stats.triangles_rejected,
                stats.pixels_shaded,
            );
        }
    }

    /// Renders one frame and presents it.
    pub fn render(&mut self) {
        let mut buffer = match self.surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(e) => {
                eprintln!("[engine] could not map the framebuffer: {e}");
                return;
            }
        };

        self.renderer.render(&mut buffer, &self.scene, &self.assets);

        if let Err(e) = buffer.present() {
            eprintln!("[engine] present failed: {e}");
        }
    }

    /// Applies the runtime debug toggles. Unmapped keys are ignored.
    fn handle_toggle(&mut self, code: KeyCode) {
        let options = &mut self.renderer.options;
        match code {
            KeyCode::KeyF => {
                options.wireframe = !options.wireframe;
                println!("[engine] wireframe: {}", options.wireframe);
            }
            KeyCode::KeyC => {
                options.backface_culling = !options.backface_culling;
                println!("[engine] back-face culling: {}", options.backface_culling);
            }
            KeyCode::KeyT => {
                options.debug_triangle_tint = !options.debug_triangle_tint;
                println!("[engine] triangle tint: {}", options.debug_triangle_tint);
            }
            KeyCode::KeyR => {
                *self.scene.camera_mut() = self.camera_reset;
                self.camera_manually_adjusted = false;
                println!("[engine] camera reset");
            }
            KeyCode::KeyH => println!("{}", EngineConfig::usage()),
            _ => {}
        }
    }
}

/// Hides and locks the cursor so mouse look does not run into the screen edge.
///
/// Not every platform supports [`CursorGrabMode::Locked`] (X11 in particular),
/// so it falls back to confining the cursor to the window, and to nothing at
/// all if that fails too — a missing grab degrades the experience, it does not
/// break the engine.
fn grab_cursor(window: &Window) {
    window.set_cursor_visible(false);
    if window.set_cursor_grab(CursorGrabMode::Locked).is_err()
        && window.set_cursor_grab(CursorGrabMode::Confined).is_err()
    {
        eprintln!("[engine] cursor grab unavailable on this platform; mouse look may escape");
    }
}

/// The camera the scene starts with, and the one `R` restores.
fn default_camera() -> Camera {
    Camera {
        pitch: CAMERA_PITCH, // look slightly down at the ring
        ..Camera::new(Vec3d::new(0.0, CAMERA_HEIGHT, CAMERA_DISTANCE))
    }
}

/// Builds the demo scene: one object at the origin plus a ring of four
/// smaller ones, each spinning at its own rate.
///
/// The point is to exercise the parts that a single hard-coded mesh could not:
/// several entities sharing one mesh handle, independent transforms, and a
/// depth buffer that has to resolve objects overlapping on screen.
///
/// `bounding_radius` is the mesh's own size: every object is scaled to unit
/// radius so the framing is the same whether the model is a half-unit cube or
/// a teapot several units across.
fn build_demo_scene(mesh: MeshHandle, texture: TextureHandle, bounding_radius: f64) -> Scene {
    /// Tint and spin rate for each satellite.
    const RING: [(u32, f64); 4] = [
        (0x00FF_8080, 0.9),
        (0x0080_FF80, -1.3),
        (0x0080_80FF, 1.7),
        (0x00FF_FF80, -0.7),
    ];

    // Normalizes any model to unit radius; a degenerate mesh falls back to 1.
    let unit = if bounding_radius > f64::EPSILON {
        1.0 / bounding_radius
    } else {
        1.0
    };

    let mut scene = Scene::new();
    *scene.camera_mut() = default_camera();

    let center = scene.spawn_object(
        Transform::default().with_uniform_scale(unit),
        MeshRenderer::new(mesh, texture),
    );
    scene.world.insert(center, Spin(Vec3d::new(0.0, 0.6, 0.0)));

    for (i, (tint, spin)) in RING.iter().enumerate() {
        let angle = i as f64 * std::f64::consts::FRAC_PI_2;
        let position = Vec3d::new(RING_RADIUS * angle.cos(), 0.0, RING_RADIUS * angle.sin());

        let satellite = scene.spawn_object(
            Transform::from_translation(position).with_uniform_scale(RING_SCALE * unit),
            MeshRenderer::new(mesh, texture).with_tint(*tint),
        );
        scene
            .world
            .insert(satellite, Spin(Vec3d::new(*spin, *spin * 0.5, 0.0)));
    }

    scene
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cena de demonstração tem o objeto central mais o anel.
    #[test]
    fn demo_scene_has_five_objects() {
        let mut assets = Assets::new();
        let mesh = assets.add_mesh(crate::assets::Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let scene = build_demo_scene(mesh, texture, 1.0);
        // 6: os cinco objetos da demo, mais a câmera, que já nasce com a cena.
        assert_eq!(scene.len(), 6);
        assert_eq!(scene.drawable_count(), 5);
    }

    /// Todo objeto da demo gira — o `Spin` é componente, não campo do bundle.
    #[test]
    fn every_demo_object_spins() {
        let mut assets = Assets::new();
        let mesh = assets.add_mesh(crate::assets::Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let scene = build_demo_scene(mesh, texture, 1.0);
        assert_eq!(scene.world.count::<Spin>(), 5);
    }

    /// Só os satélites giram com tint próprio; o central fica neutro.
    #[test]
    fn only_the_satellites_are_tinted() {
        let mut assets = Assets::new();
        let mesh = assets.add_mesh(crate::assets::Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let scene = build_demo_scene(mesh, texture, 1.0);
        let tinted = scene
            .drawables()
            .filter(|(_, renderer)| renderer.tint != crate::scene::NO_TINT)
            .count();
        assert_eq!(tinted, 4);
    }

    /// A câmera padrão fica atrás e acima da origem, olhando para baixo.
    #[test]
    fn default_camera_looks_at_the_ring() {
        let camera = default_camera();
        assert!(camera.position.z() > 0.0);
        assert!(camera.pitch < 0.0);
    }
}
