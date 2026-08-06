//! Frame timing and the throttled performance report.

use std::time::{Duration, Instant};

/// Upper bound on a single frame's delta time, in seconds.
///
/// Without it, a stall (dragging the window, waking from sleep) produces a
/// huge `dt` that teleports the camera through the scene. Clamping trades
/// exact simulation time for stability — the standard "spiral of death" guard.
const MAX_DELTA_SECONDS: f64 = 0.25;

/// How often the engine prints a performance line.
const REPORT_INTERVAL: Duration = Duration::from_secs(1);

/// A snapshot of engine performance, emitted at most once per second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameReport {
    /// Total frames rendered since startup.
    pub frame: u64,
    /// Average frames per second over the reporting window.
    pub fps: f64,
    /// Average frame time over the window, in milliseconds.
    pub frame_time_ms: f64,
}

/// Measures frame deltas and decides when a report is due.
///
/// Averaging over a fixed window instead of reporting the instantaneous `1/dt`
/// of one arbitrary frame gives a number that is actually comparable between
/// runs — which is the point of measuring at all.
#[derive(Debug)]
pub struct FrameClock {
    /// Instant the previous frame started.
    last_frame: Instant,
    /// Frames rendered since startup.
    frame_count: u64,
    /// Time accumulated in the current reporting window.
    window_elapsed: Duration,
    /// Frames counted in the current reporting window.
    window_frames: u32,
}

impl Default for FrameClock {
    /// Starts the clock at the current instant.
    fn default() -> Self {
        Self::new()
    }
}

impl FrameClock {
    /// Starts the clock at the current instant.
    pub fn new() -> Self {
        Self {
            last_frame: Instant::now(),
            frame_count: 0,
            window_elapsed: Duration::ZERO,
            window_frames: 0,
        }
    }

    /// Closes the previous frame and returns its duration in seconds, clamped
    /// to a quarter of a second.
    pub fn tick(&mut self) -> f64 {
        let now = Instant::now();
        let elapsed = now - self.last_frame;
        self.last_frame = now;

        self.frame_count += 1;
        self.window_elapsed += elapsed;
        self.window_frames += 1;

        elapsed.as_secs_f64().min(MAX_DELTA_SECONDS)
    }

    /// Returns a report once per second, resetting the window.
    ///
    /// Returns `None` on every other frame, so the caller can log
    /// unconditionally without flooding the terminal.
    pub fn take_report(&mut self) -> Option<FrameReport> {
        if self.window_elapsed < REPORT_INTERVAL || self.window_frames == 0 {
            return None;
        }

        let seconds = self.window_elapsed.as_secs_f64();
        let frames = self.window_frames as f64;
        let report = FrameReport {
            frame: self.frame_count,
            fps: frames / seconds,
            frame_time_ms: seconds / frames * 1000.0,
        };

        self.window_elapsed = Duration::ZERO;
        self.window_frames = 0;
        Some(report)
    }

    /// Total frames rendered since startup.
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um tick devolve um dt finito e não negativo.
    #[test]
    fn tick_returns_a_sane_delta() {
        let mut clock = FrameClock::new();
        let dt = clock.tick();

        assert!((0.0..=MAX_DELTA_SECONDS).contains(&dt));
        assert_eq!(clock.frame_count(), 1);
    }

    /// O relatório só sai depois da janela fechar — não a cada frame.
    #[test]
    fn report_is_throttled() {
        let mut clock = FrameClock::new();
        clock.tick();
        assert!(clock.take_report().is_none());

        clock.window_elapsed = REPORT_INTERVAL;
        clock.window_frames = 60;
        let report = clock.take_report().expect("a janela fechou");

        assert!((report.fps - 60.0).abs() < 1e-9);
        assert!((report.frame_time_ms - 1000.0 / 60.0).abs() < 1e-9);
        assert!(clock.take_report().is_none(), "a janela foi reiniciada");
    }
}
