use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use softbuffer::{Context, Surface};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::window::Window;

use crate::math::utils::{Mat4x4, Vec3d, Vec4d};
use crate::texture::mesh::Mesh;

/// Núcleo da engine: dona da janela, do framebuffer e do loop de frame.
/// Vai crescer para segurar cena, câmera etc. conforme `math`, `clipper`
/// e `texture` forem ganhando implementação.
pub struct Engine {
    window: Rc<Window>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    last_frame: Instant,
    width: u32,
    height: u32,
    mesh: Mesh,
    theta: f64,
    proj: Mat4x4,
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
            width: 0,
            height: 0,
            mesh: Mesh::load_from_obj("assets/cube.obj"),
            theta: 0.0,
            proj: Mat4x4::identity(),
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

        self.width = size.width;
        self.height = size.height;
        let aspect = size.width as f64 / size.height as f64;
        self.proj = Mat4x4::perpective(90.0_f64.to_radians(), aspect, 0.1, 1000.0)
    }

    /// Uma vez por frame, antes do render.
    /// Aqui entra: física, animação, atualização de câmera — usando `dt`.
    pub fn update(&mut self) {
        let dt = self.last_frame.elapsed();
        self.last_frame = Instant::now();
        self.theta += dt.as_secs_f64();
    }

    /// Uma vez por frame, depois do update. `buffer` é o framebuffer bruto
    /// (u32 por pixel, 0x00RRGGBB) — é aqui que seu rasterizador escreve:
    /// transformar vértices (math), recortar (clipper::clipping),
    /// rasterizar e texturizar (texture::mesh / texture::texturing).
    pub fn render(&mut self) {
        let mut buffer = self.surface.buffer_mut().expect("failed to get buffer");
        buffer.fill(0x000000); // clear de tela — troque pelo desenho da cena

        let world =
            Mat4x4::translation(Vec3d::new(0.0, 0.0, -3.0)) * Mat4x4::rotation_y(self.theta);
        for tri in self.mesh.indices.chunks_exact(3) {
            let screen_pts: Vec<(i64, i64)> = tri
                .iter()
                .map(|&idx| {
                    let p = self.mesh.vertices[idx as usize].position;
                    let view_space = world * Vec4d::new(p[0] as f64, p[1] as f64, p[2] as f64, 1.0);
                    let clip = self.proj * view_space;

                    let ndc_x = clip.x() / clip.w();
                    let ndc_y = clip.y() / clip.w();
                    let x = (ndc_x + 1.0) * 0.5 * self.width as f64;
                    let y = (1.0 - ndc_y) * 0.5 * self.height as f64;
                    (x as i64, y as i64)
                })
                .collect();
            draw_line(
                &mut buffer,
                self.width,
                self.height,
                screen_pts[0],
                screen_pts[1],
            );
            draw_line(
                &mut buffer,
                self.width,
                self.height,
                screen_pts[1],
                screen_pts[2],
            );
            draw_line(
                &mut buffer,
                self.width,
                self.height,
                screen_pts[2],
                screen_pts[0],
            );
        }

        buffer.present().expect("failed to present buffer");
    }

    /// Eventos de teclado/mouse (tudo que não é close/resize/redraw).
    pub fn input(&mut self, event: WindowEvent) {
        let _ = event;
    }
}

/// Desenha só pra ver a malha na tela. Vira rasterização de
/// triângulo preenchido no próximo passo.
fn draw_line(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    (mut x0, mut y0): (i64, i64),
    (x1, y1): (i64, i64),
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if x0 >= 0 && y0 >= 0 && (x0 as u32) < width && (y0 as u32) < height {
            buffer[y0 as usize * width as usize + x0 as usize] = 0x00FFFFFF;
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}
