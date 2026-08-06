//! Platform layer: window, input, frame timing and the loop that ties the
//! other modules together.

pub mod clock;
pub mod config;
pub mod core;
pub mod input;

pub use clock::{FrameClock, FrameReport};
pub use config::EngineConfig;
pub use core::Engine;
pub use input::InputState;
