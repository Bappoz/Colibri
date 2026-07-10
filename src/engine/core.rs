use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use softbuffer::{Context, Surface};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::window::Window;

/// Núcleo da engine: dona da janela, do framebuffer e do loop de frame.
/// Vai crescer para segurar cena, câmera etc. conforme `math`, `clipper`
/// e `texture` forem ganhando implementação.
pub struct Engine {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    last_frame: Instant,
}

impl Engine {
    pub fn new(window: Window) -> Self {
        let window = Rc::new(window);
        let context = Context::new(window.clone()).expect("failed to create softbuffer context");
        let surface = Surface::new(&context, window.clone()).expect("failed to create surface");

        let mut engine = Self {
            window,
            surface,
            last_frame: Instant::now(),
        };
        // Sem isso a surface fica com tamanho 0 e o primeiro frame nunca aparece.
        let size = engine.window.inner_size();
        engine.resize(size);
        engine
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Redimensionamento da janela: realoca o framebuffer no novo tamanho.
    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return; // janela minimizada (0x0) — nada a fazer
        };
        self.surface
            .resize(width, height)
            .expect("failed to resize surface");
    }

    /// Uma vez por frame, antes do render.
    /// Aqui entra: física, animação, atualização de câmera — usando `dt`.
    pub fn update(&mut self) {
        let dt = self.last_frame.elapsed();
        self.last_frame = Instant::now();
        let _ = dt;
    }

    /// Uma vez por frame, depois do update. `buffer` é o framebuffer bruto
    /// (u32 por pixel, 0x00RRGGBB) — é aqui que seu rasterizador escreve:
    /// transformar vértices (math), recortar (clipper::clipping),
    /// rasterizar e texturizar (texture::mesh / texture::texturing).
    pub fn render(&mut self) {
        let mut buffer = self.surface.buffer_mut().expect("failed to get buffer");
        buffer.fill(0x00202020); // clear de tela — troque pelo desenho da cena
        buffer.present().expect("failed to present buffer");
    }

    /// Eventos de teclado/mouse (tudo que não é close/resize/redraw).
    pub fn input(&mut self, event: WindowEvent) {
        let _ = event;
    }
}
