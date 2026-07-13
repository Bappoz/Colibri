mod app;

use app::App;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --triangles (ou -t): colore cada triangulo individualmente em vez de por face.
    let per_triangle_shading = std::env::args().any(|arg| arg == "--triangles" || arg == "-t");

    let event_loop = EventLoop::new()?;
    // Poll em vez de Wait: uma engine 3D precisa desenhar todo frame,
    // não só quando um evento de SO chega.
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(per_triangle_shading);
    event_loop.run_app(&mut app)?;
    Ok(())
}
