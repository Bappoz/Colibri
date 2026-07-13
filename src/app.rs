use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use colibri::engine::Engine;

/// Ponte entre o winit e a Engine. Não tem lógica de 3D — só roteia eventos.
pub struct App {
    engine: Option<Engine>,
    per_triangle_shading: bool,
}

impl App {
    pub fn new(per_triangle_shading: bool) -> Self {
        Self {
            engine: None,
            per_triangle_shading,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = Window::default_attributes()
            .with_title("rustic_3d_engine")
            .with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));

        let window = event_loop
            .create_window(window_attributes)
            .expect("failed to create window");

        self.engine = Some(Engine::new(window, self.per_triangle_shading));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        if window_id != engine.window().id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                println!("Encerrando...");
                event_loop.exit();
            }
            WindowEvent::Resized(size) => engine.resize(size),
            WindowEvent::RedrawRequested => engine.render(),
            other => engine.input(other),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(engine) = self.engine.as_mut() else {
            return;
        };
        engine.update();
        engine.window().request_redraw();
    }
}
