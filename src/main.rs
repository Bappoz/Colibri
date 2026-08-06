//! Binary entry point: parses the command line and starts the event loop.
//!
//! All the engine logic lives in the `colibri` library, so it can be driven
//! from tests and examples without a window. Run with `--help` for the flags
//! and the controls.

mod app;

use std::process::ExitCode;

use app::App;
use colibri::engine::EngineConfig;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> ExitCode {
    let config = match EngineConfig::from_args(std::env::args().skip(1)) {
        Ok(Some(config)) => config,
        // `--help`: printing the usage is the whole job.
        Ok(None) => {
            print!("{}", EngineConfig::usage());
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("colibri: {message}");
            return ExitCode::FAILURE;
        }
    };

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("colibri: could not create the event loop: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Poll, not Wait: a 3D engine draws every frame, not only when the OS
    // sends an event.
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(config);
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("colibri: event loop failed: {e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
