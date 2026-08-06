//! Bridges `winit`'s event-driven input to the polling the frame loop wants.

use std::collections::HashSet;

use winit::keyboard::KeyCode;

/// Keyboard and mouse state accumulated between frames.
///
/// `winit` reports *transitions* ("W went down"), but a frame needs *state*
/// ("is W down right now?"). Holding the set here is what lets
/// [`crate::scene::Camera::process_keyboard`] move by `speed * dt` once per
/// frame instead of once per event — which is also what makes movement speed
/// independent of the event rate.
#[derive(Debug, Default, Clone)]
pub struct InputState {
    /// Keys currently held down.
    held: HashSet<KeyCode>,
    /// Mouse movement summed since the last [`InputState::take_mouse_delta`],
    /// in raw device units.
    mouse_delta: (f64, f64),
}

impl InputState {
    /// An empty state: nothing held, no pending mouse movement.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `key` is currently held down.
    pub fn is_held(&self, key: KeyCode) -> bool {
        self.held.contains(&key)
    }

    /// Records a key going down. Idempotent, so keyboard auto-repeat is
    /// harmless.
    pub fn key_down(&mut self, key: KeyCode) {
        self.held.insert(key);
    }

    /// Records a key coming back up.
    pub fn key_up(&mut self, key: KeyCode) {
        self.held.remove(&key);
    }

    /// Adds a raw mouse movement to the pending delta.
    ///
    /// Several motion events usually arrive per frame; summing them means no
    /// movement is dropped just because the frame boundary fell in between.
    pub fn accumulate_mouse(&mut self, dx: f64, dy: f64) {
        self.mouse_delta.0 += dx;
        self.mouse_delta.1 += dy;
    }

    /// Returns the accumulated mouse movement and resets it to zero.
    ///
    /// Consuming rather than peeking is deliberate: a delta must be applied
    /// exactly once, or the camera keeps turning on its own.
    pub fn take_mouse_delta(&mut self) -> (f64, f64) {
        std::mem::take(&mut self.mouse_delta)
    }

    /// Releases every key.
    ///
    /// Needed when the window loses focus: the key-up event is delivered to
    /// whoever has focus now, so without this the camera would keep drifting.
    pub fn release_all(&mut self) {
        self.held.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma tecla fica "segurada" entre o down e o up.
    #[test]
    fn keys_stay_held_between_events() {
        let mut input = InputState::new();
        assert!(!input.is_held(KeyCode::KeyW));

        input.key_down(KeyCode::KeyW);
        input.key_down(KeyCode::KeyW); // auto-repeat
        assert!(input.is_held(KeyCode::KeyW));

        input.key_up(KeyCode::KeyW);
        assert!(!input.is_held(KeyCode::KeyW));
    }

    /// Movimentos de mouse somam no frame e o delta é consumido uma só vez.
    #[test]
    fn mouse_deltas_accumulate_then_drain() {
        let mut input = InputState::new();
        input.accumulate_mouse(2.0, -1.0);
        input.accumulate_mouse(3.0, 4.0);

        assert_eq!(input.take_mouse_delta(), (5.0, 3.0));
        assert_eq!(input.take_mouse_delta(), (0.0, 0.0), "não pode repetir");
    }

    /// Perder o foco solta tudo — senão a câmera anda sozinha.
    #[test]
    fn losing_focus_releases_every_key() {
        let mut input = InputState::new();
        input.key_down(KeyCode::KeyW);
        input.key_down(KeyCode::ShiftLeft);

        input.release_all();

        assert!(!input.is_held(KeyCode::KeyW));
        assert!(!input.is_held(KeyCode::ShiftLeft));
    }
}
