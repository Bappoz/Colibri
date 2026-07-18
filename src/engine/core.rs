use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use softbuffer::{Context, Surface};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::window::Window;

use crate::clipper::clipping::clip_triangle_near;
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
    depth: Vec<f32>,
    frame_count: u64,
    per_triangle_shading: bool,
}

#[derive(Clone, Copy)]
pub struct Vertex2D {
    x: f64,
    y: f64,
    z: f64,
}

const FACE_COLORS: [u32; 6] = [
    0x00FF0000, 0x0000FF00, 0x000000FF, 0x00FFFF00, 0x00FF00FF, 0x0000FFFF,
];

impl Engine {
    pub fn new(window: Window, per_triangle_shading: bool) -> Self {
        let window = Rc::new(window);
        let context = Context::new(window.clone()).expect("failed to create softbuffer context");
        let surface = Surface::new(&context, window.clone()).expect("failed to create surface");

        let mesh = Mesh::load_from_obj("assets/VideoShip.obj");
        println!(
            "[engine] mesh carregada: {} vertices, {} triangulos",
            mesh.vertices.len(),
            mesh.indices.len() / 3
        );
        println!(
            "[engine] modo de shading: {}",
            if per_triangle_shading {
                "por triangulo"
            } else {
                "por face (completo)"
            }
        );

        let mut engine = Self {
            window,
            surface,
            last_frame: Instant::now(),
            width: 0,
            height: 0,
            mesh,
            theta: 0.0,
            proj: Mat4x4::identity(),
            depth: Vec::new(),
            frame_count: 0,
            per_triangle_shading,
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

        self.depth = vec![f32::INFINITY; (self.width * self.height) as usize];

        let aspect = size.width as f64 / size.height as f64;
        self.proj = Mat4x4::perpective(90.0_f64.to_radians(), aspect, 0.1, 1000.0);

        println!(
            "[engine] resize -> {}x{} (depth buffer: {} px, aspect: {:.3})",
            self.width,
            self.height,
            self.depth.len(),
            aspect
        );
    }

    /// Uma vez por frame, antes do render.
    /// Aqui entra: física, animação, atualização de câmera — usando `dt`.
    pub fn update(&mut self) {
        let dt = self.last_frame.elapsed();
        self.last_frame = Instant::now();
        self.theta += dt.as_secs_f64();

        self.frame_count += 1;
        if self.frame_count.is_multiple_of(120) {
            let fps = if dt.as_secs_f64() > 0.0 {
                1.0 / dt.as_secs_f64()
            } else {
                0.0
            };
            println!(
                "[engine] frame {} | dt: {:.2}ms | fps: {:.1} | theta: {:.2}rad",
                self.frame_count,
                dt.as_secs_f64() * 1000.0,
                fps,
                self.theta
            );
        }
    }

    /// Uma vez por frame, depois do update. `buffer` é o framebuffer bruto
    /// (u32 por pixel, 0x00RRGGBB) — é aqui que seu rasterizador escreve:
    /// transformar vértices (math), recortar (clipper::clipping),
    /// rasterizar e texturizar (texture::mesh / texture::texturing).
    pub fn render(&mut self) {
        let mut buffer = self.surface.buffer_mut().expect("failed to get buffer");
        buffer.fill(0x000000); // clear de tela — troque pelo desenho da cena
        self.depth.fill(f32::INFINITY); // Reset por frame o depth

        let world =
            Mat4x4::translation(Vec3d::new(0.0, 0.0, -3.0)) * Mat4x4::rotation_y(self.theta);

        let mut drawn = 0u32;
        let mut culled = 0u32;

        for (face_idx, tri) in self.mesh.indices.chunks_exact(3).enumerate() {
            // Transforma os vertices originais para o ClipSpace antes de dividir por W
            let mut clip_vertices = [Vec4d::new(0.0, 0.0, 0.0, 0.0); 3];
            for (i, &idx) in tri.iter().enumerate() {
                let p = self.mesh.vertices[idx as usize].position;
                let view_space = world * Vec4d::new(p[0] as f64, p[1] as f64, p[2] as f64, 1.0);
                clip_vertices[i] = self.proj * view_space;
            }

            // Executa o clipping contra o plano Near
            let clipped_triangles =
                clip_triangle_near(clip_vertices[0], clip_vertices[1], clip_vertices[2]);

            // Processa todos os triangulos resultantes do Clipping
            for tri_vertices in clipped_triangles {
                let screen_pts: [Vertex2D; 3] = [
                    Vertex2D {
                        x: (tri_vertices[0].x() / tri_vertices[0].w() + 1.0)
                            * 0.5
                            * self.width as f64,
                        y: (1.0 - tri_vertices[0].y() / tri_vertices[0].w())
                            * 0.5
                            * self.height as f64,
                        z: (tri_vertices[0].z() / tri_vertices[0].w()),
                    },
                    Vertex2D {
                        x: (tri_vertices[1].x() / tri_vertices[1].w() + 1.0)
                            * 0.5
                            * self.width as f64,
                        y: (1.0 - tri_vertices[1].y() / tri_vertices[1].w())
                            * 0.5
                            * self.height as f64,
                        z: (tri_vertices[1].z() / tri_vertices[1].w()),
                    },
                    Vertex2D {
                        x: (tri_vertices[2].x() / tri_vertices[2].w() + 1.0)
                            * 0.5
                            * self.width as f64,
                        y: (1.0 - tri_vertices[2].y() / tri_vertices[2].w())
                            * 0.5
                            * self.height as f64,
                        z: (tri_vertices[2].z() / tri_vertices[2].w()),
                    },
                ];

                let area = edge(
                    screen_pts[0].x,
                    screen_pts[0].y,
                    screen_pts[1].x,
                    screen_pts[1].y,
                    screen_pts[2].x,
                    screen_pts[2].y,
                );

                if area >= 0.0 {
                    culled += 1;
                    continue;
                }

                // Por padrao colore por face (par de triangulos); com a flag, cada triangulo tem sua propria cor.
                let color_idx = if self.per_triangle_shading {
                    face_idx
                } else {
                    face_idx / 2
                };
                let shade = FACE_COLORS[color_idx % FACE_COLORS.len()];

                if raster_triangle(
                    &mut buffer,
                    self.width,
                    self.height,
                    &mut self.depth,
                    [screen_pts[0], screen_pts[1], screen_pts[2]],
                    shade,
                ) {
                    drawn += 1;
                } else {
                    culled += 1;
                }
            }
        }

        if self.frame_count.is_multiple_of(120) {
            println!("[render] triangulos desenhados: {drawn} | culled: {culled}");
        }

        buffer.present().expect("failed to present buffer");
    }

    /// Eventos de teclado/mouse (tudo que não é close/resize/redraw).
    pub fn input(&mut self, event: WindowEvent) {
        let _ = event;
    }
}

fn edge(ax: f64, ay: f64, bx: f64, by: f64, px: f64, py: f64) -> f64 {
    (bx - ax) * (py - ay) - (by - ay) * (px - ax)
}

/// Desenha os triangulos rastreando os mais proximos da tela e os mais distantes.
/// Retorna `false` quando o triangulo foi culled (fora da tela ou degenerado).
fn raster_triangle(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    depth: &mut [f32],
    v: [Vertex2D; 3],
    shade: u32,
) -> bool {
    let min_x = v
        .iter()
        .map(|p| p.x)
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0) as i64;
    let max_x = v
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(width as f64 - 1.0) as i64;
    let min_y = v
        .iter()
        .map(|p| p.y)
        .fold(f64::INFINITY, f64::min)
        .floor()
        .max(0.0) as i64;
    let max_y = v
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil()
        .min(height as f64 - 1.0) as i64;
    if min_x > max_x || min_y > max_y {
        return false; // Bounding box fora da tela
    }

    let area = edge(v[0].x, v[0].y, v[1].x, v[1].y, v[2].x, v[2].y);
    if area == 0.0 {
        return false; // Triangulo degenerado
    }

    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let (fx, fy) = (px as f64 + 0.5, py as f64 + 0.5);
            let w0 = edge(v[1].x, v[1].y, v[2].x, v[2].y, fx, fy) / area;
            let w1 = edge(v[2].x, v[2].y, v[0].x, v[0].y, fx, fy) / area;
            let w2 = edge(v[0].x, v[0].y, v[1].x, v[1].y, fx, fy) / area;

            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }

            let z = (w0 * v[0].z + w1 * v[1].z + w2 * v[2].z) as f32;
            let i = py as usize * width as usize + px as usize;
            if z < depth[i] {
                depth[i] = z;
                buffer[i] = shade;
            }
        }
    }

    true
}
