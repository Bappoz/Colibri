//! The engine: owns the window, the surface, and the update/render loop.
//!
//! Everything the frame actually needs lives elsewhere — the scene in
//! [`crate::scene`], the pixels in [`crate::render`], the data in
//! [`crate::assets`]. What is left here is the platform plumbing and the order
//! the pieces run in.

use std::num::NonZeroU32;
use std::rc::Rc;

use softbuffer::{Context, Surface};
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

use crate::assets::{Assets, MeshHandle, Texture, TextureHandle};
use crate::engine::clock::FrameClock;
use crate::engine::config::EngineConfig;
use crate::engine::input::InputState;
use crate::error::{Error, Result};
use crate::math::Vec3d;
use crate::render::Renderer;
use crate::scene::{Camera, RenderObject, Scene, Transform};

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
        grab_cursor(&window);

        let context = Context::new(window.clone())
            .map_err(|e| Error::Surface(format!("softbuffer context: {e}")))?;
        let surface = Surface::new(&context, window.clone())
            .map_err(|e| Error::Surface(format!("softbuffer surface: {e}")))?;

        let mut assets = Assets::new();
        let mesh = assets.load_mesh(&config.model_path)?;
        let texture = match config.texture_path.as_deref() {
            Some(path) => assets.load_texture(path)?,
            None => assets.add_texture(Texture::checkerboard(CHECKER_SIZE, CHECKER_CELL)),
        };

        let scene = build_demo_scene(mesh, texture, assets.mesh(mesh).bounding_radius());

        println!(
            "[engine] loaded '{}': {} vertices, {} triangles",
            config.model_path,
            assets.mesh(mesh).vertices.len(),
            assets.total_triangles()
        );
        if !assets.mesh(mesh).has_texture_coords() {
            println!(
                "[engine] note: '{}' has no UV data — the texture samples a single texel, \
                 so the surface shows only the lighting",
                config.model_path
            );
        }
        println!("[engine] scene: {} objects", scene.len());
        println!("[engine] H prints the controls; F/C/T toggle the debug views");

        let mut engine = Self {
            window,
            surface,
            renderer: Renderer::new(config.render),
            scene,
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

        let (dx, dy) = self.input.take_mouse_delta();
        self.scene
            .camera
            .process_mouse(dx, dy, self.config.mouse_sensitivity);
        self.scene
            .camera
            .process_keyboard(&self.input, self.config.move_speed, dt);

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
                self.scene.camera = default_camera();
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
    scene.camera = default_camera();

    scene.spawn(
        RenderObject::new(Transform::default().with_uniform_scale(unit), mesh, texture)
            .with_angular_velocity(Vec3d::new(0.0, 0.6, 0.0)),
    );

    for (i, (tint, spin)) in RING.iter().enumerate() {
        let angle = i as f64 * std::f64::consts::FRAC_PI_2;
        let position = Vec3d::new(RING_RADIUS * angle.cos(), 0.0, RING_RADIUS * angle.sin());

        scene.spawn(
            RenderObject::new(
                Transform::from_translation(position).with_uniform_scale(RING_SCALE * unit),
                mesh,
                texture,
            )
            .with_tint(*tint)
            .with_angular_velocity(Vec3d::new(*spin, *spin * 0.5, 0.0)),
        );
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
        assert_eq!(scene.len(), 5);
    }

    /// Só os satélites giram com tint próprio; o central fica neutro.
    #[test]
    fn only_the_satellites_are_tinted() {
        let mut assets = Assets::new();
        let mesh = assets.add_mesh(crate::assets::Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let scene = build_demo_scene(mesh, texture, 1.0);
        let tinted = scene
            .objects()
            .filter(|o| o.tint != crate::scene::NO_TINT)
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
