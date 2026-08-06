//! Software rendering: clipping, rasterization and the frame pipeline.
//!
//! Everything here runs on the CPU — there is no GPU anywhere in Colibri yet.
//! The stages, in the order a vertex meets them:
//!
//! 1. [`renderer`] — transforms vertices and walks the index buffer.
//! 2. [`clip`] — rejects off-screen triangles and cuts the near plane.
//! 3. [`raster`] — perspective divide, back-face test, and the pixel loop.
//! 4. [`target`] — the color and depth buffers being written.

pub mod clip;
pub mod raster;
pub mod renderer;
pub mod target;

pub use clip::{ClipOutput, ClipVertex};
pub use raster::ScreenVertex;
pub use renderer::{RenderOptions, RenderStats, Renderer};
pub use target::RenderTarget;
