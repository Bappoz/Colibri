use std::collections::HashSet;
use winit::keyboard::KeyCode;

// Estado de teclado/mouse acumulado em frames
#[derive(Default)]
pub struct InputState {
    held: HashSet<KeyCode>,
    mouse_delta: (f64, f64),
}

impl InputState {
    pub fn is_held(&self, key: KeyCode) -> bool {
        self.held.contains(&key)
    }

    pub fn key_down(&mut self, key: KeyCode) {
        self.held.insert(key);
    }

    pub fn key_up(&mut self, key: KeyCode) {
        self.held.remove(&key);
    }

    pub fn accumulate_mouse(&mut self, dx: f64, dy: f64) {
        self.mouse_delta.0 += dx;
        self.mouse_delta.1 += dy;
    }

    pub fn take_mouse_delta(&mut self) -> (f64, f64) {
        std::mem::take(&mut self.mouse_delta)
    }
}
