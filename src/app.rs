//! `winit` application shell.
//!
//! Holds no 3D logic: it creates the window when the platform is ready and
//! forwards events to the [`Engine`]. Keeping it this thin is what lets the
//! engine be driven from a test with no window at all.

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

use colibri::engine::{Engine, EngineConfig};

/// Application state across the two phases of a `winit` run: before the
/// window exists, and after.
pub struct App {
    /// `None` until the event loop resumes and the window can be created.
    engine: Option<Engine>,
    /// Configuration parsed from the command line.
    config: EngineConfig,
}

impl App {
    /// Creates the shell. No window and no engine exist yet — `winit` decides
    /// when that is allowed.
    pub fn new(config: EngineConfig) -> Self {
        Self {
            engine: None,
            config,
        }
    }
}

impl ApplicationHandler for App {
    /// Creates the window and the engine.
    ///
    /// May be called more than once on mobile platforms; on desktop it fires
    /// once, right after the loop starts.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.engine.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("colibri")
            .with_fullscreen(Some(Fullscreen::Borderless(None)));

        let window = match event_loop.create_window(attributes) {
            Ok(window) => window,
            Err(e) => {
                eprintln!("colibri: could not create a window: {e}");
                event_loop.exit();
                return;
            }
        };

        match Engine::new(window, self.config.clone()) {
            Ok(engine) => self.engine = Some(engine),
            Err(e) => {
                // Asset failures land here; there is nothing to draw, so say
                // what is missing and quit rather than showing a blank window.
                eprintln!("colibri: {e}");
                event_loop.exit();
            }
        }
    }

    /// Forwards raw mouse motion.
    ///
    /// Device events are used instead of `CursorMoved` on purpose: they report
    /// unaccelerated deltas that keep working once the cursor is locked in
    /// place, which is exactly what mouse look needs.
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        if let DeviceEvent::MouseMotion { delta } = event {
            engine.mouse_moved(delta);
        }
    }

    /// Handles the events the shell owns (quit, resize, redraw) and passes
    /// everything else to the engine.
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        if window_id != engine.window().id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                println!("[app] shutting down");
                event_loop.exit();
            }
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => event_loop.exit(),
            WindowEvent::Resized(size) => engine.resize(size),
            WindowEvent::RedrawRequested => engine.render(),
            other => engine.handle_window_event(other),
        }
    }

    /// Drives the frame loop.
    ///
    /// With `ControlFlow::Poll` this runs as fast as the machine allows, which
    /// is what a renderer wants: simulate, then ask for a redraw.
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        engine.update();
        engine.window().request_redraw();
    }
}
