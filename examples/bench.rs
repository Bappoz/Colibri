//! Headless render benchmark.
//!
//! Renders a fixed number of frames into a plain pixel buffer — no window, no
//! event loop — and reports the throughput. It exists so that "this is faster"
//! is a measurement rather than an impression, and it is the harness to run
//! before and after touching [`colibri::render`].
//!
//! ```bash
//! cargo run --release --example bench                                    # defaults
//! cargo run --release --example bench -- assets/teapot.obj 200           # model, frames
//! cargo run --release --example bench -- assets/teapot.obj 200 640 360   # ...and viewport
//! cargo run --release --example bench -- assets/cube.obj 200 1920 1080 1.1  # ...and zoom
//! cargo run --release --example bench -- assets/cube.obj 200 1920 1080 1.1 1 # ...and threads
//! ```
//!
//! The last argument is the camera distance as a multiple of the model's
//! bounding radius: `2.5` frames the whole model, `1.1` puts the camera right
//! up against it. The low-zoom case is the one worth optimizing — it is what
//! the engine does when you fly into the geometry.
//!
//! Always run it with `--release`: a debug build of a software rasterizer is
//! roughly an order of magnitude slower and measures nothing useful.

use std::time::Instant;

use colibri::assets::{Assets, Texture};
use colibri::math::Vec3d;
use colibri::render::{RenderOptions, Renderer};
use colibri::scene::{Camera, MeshRenderer, Scene, Spin, Transform};

/// Viewport width used when none is given, matching a common 1080p window.
const DEFAULT_WIDTH: usize = 1920;
/// Viewport height used when none is given.
const DEFAULT_HEIGHT: usize = 1080;
/// Frames rendered when no count is given on the command line.
const DEFAULT_FRAMES: usize = 120;
/// Camera distance, as a multiple of the model's bounding radius.
const DEFAULT_ZOOM: f64 = 2.5;
/// Raster worker threads; `0` asks the platform for the core count.
const DEFAULT_THREADS: usize = 0;
/// Model used when no path is given.
const DEFAULT_MODEL: &str = "assets/teapot.obj";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let model = args.next().unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let frames: usize = parse_or(args.next(), DEFAULT_FRAMES)?;
    let width: usize = parse_or(args.next(), DEFAULT_WIDTH)?;
    let height: usize = parse_or(args.next(), DEFAULT_HEIGHT)?;
    let zoom: f64 = match args.next() {
        Some(value) => value.parse()?,
        None => DEFAULT_ZOOM,
    };
    let threads: usize = parse_or(args.next(), DEFAULT_THREADS)?;

    let mut assets = Assets::new();
    let mesh = assets.load_mesh(&model)?;
    let texture = assets.add_texture(Texture::checkerboard(256, 32));

    // The camera is placed from the mesh's own size so that every model fills
    // a comparable slice of the viewport: benchmarking a mesh that covers
    // three pixels would only measure the vertex stage. Lowering `zoom` walks
    // the camera in until the geometry fills the screen, which is the
    // fill-rate case — the one that gets slow in the real application.
    let distance = zoom * assets.mesh(mesh).bounding_radius();
    let mut scene = Scene::new();
    scene.camera = Camera::new(Vec3d::new(0.0, 0.0, distance));
    let object = scene.spawn_object(Transform::default(), MeshRenderer::new(mesh, texture));
    scene.world.insert(object, Spin(Vec3d::new(0.0, 1.0, 0.0)));

    let mut renderer = Renderer::new(RenderOptions {
        threads,
        ..RenderOptions::default()
    });
    renderer.resize(width, height);
    let mut pixels = vec![0u32; width * height];

    // One untimed frame so the first-touch page faults on the buffers do not
    // land inside the measurement.
    scene.update(1.0 / 60.0);
    renderer.render(&mut pixels, &scene, &assets);

    let start = Instant::now();
    let mut stats = renderer.stats();
    for _ in 0..frames {
        scene.update(1.0 / 60.0);
        stats = renderer.render(&mut pixels, &scene, &assets);
    }
    let elapsed = start.elapsed();

    let per_frame_ms = elapsed.as_secs_f64() * 1000.0 / frames as f64;
    println!("model            {model}");
    println!("viewport         {width}x{height}");
    println!("camera distance  {distance:.2} ({zoom:.2}x bounding radius)");
    println!("raster threads   {}", renderer.threads());
    println!("frames           {frames}");
    println!("frame time       {per_frame_ms:.2} ms");
    println!("throughput       {:.1} fps", 1000.0 / per_frame_ms);
    println!(
        "last frame       {} submitted, {} rejected, {} culled, {} drawn",
        stats.triangles_submitted,
        stats.triangles_rejected,
        stats.triangles_culled,
        stats.triangles_drawn
    );
    println!("                 {} pixels shaded", stats.pixels_shaded);

    Ok(())
}

/// Parses an optional positional argument, falling back to a default.
fn parse_or(arg: Option<String>, default: usize) -> Result<usize, std::num::ParseIntError> {
    match arg {
        Some(value) => value.parse(),
        None => Ok(default),
    }
}
