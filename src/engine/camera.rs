use crate::engine::input::InputState;
use crate::math::utils::{Mat4x4, Vec3d};
use winit::keyboard::KeyCode;

// Camera Livre
pub struct Camera {
    pub position: Vec3d,
    pub yaw: f64,
    pub pitch: f64,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            position: Vec3d::new(0.0, 0.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
        }
    }

    // Vetor unitário representado onde a "camera olha"
    pub fn forward(&self) -> Vec3d {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3d::new(-sy * cp, sp, -cy * cp)
    }

    // Sempre horizontal
    pub fn right(&self) -> Vec3d {
        self.forward().cross(&Vec3d::new(0.0, 1.0, 0.0)).normalize()
    }

    pub fn view_matrix(&self) -> Mat4x4 {
        let target = self.position + self.forward();
        Mat4x4::point_at(self.position, target, Vec3d::new(0.0, 1.0, 0.0)).quick_inverse()
    }

    pub fn process_mouse(&mut self, dx: f64, dy: f64, sensitivity: f64) {
        self.yaw += dx * sensitivity;
        let limit = 89.0_f64.to_radians(); // Evita gimbal flip
        self.pitch = (self.pitch - dy * sensitivity).clamp(-limit, limit);
    }

    pub fn process_keyboard(&mut self, input: &InputState, speed: f64, dt: f64) {
        let velocity = speed * dt;
        let (forward, right) = (self.forward(), self.right());
        if input.is_held(KeyCode::KeyW) {
            self.position = self.position + forward * velocity;
        }
        if input.is_held(KeyCode::KeyS) {
            self.position = self.position - forward * velocity;
        }

        if input.is_held(KeyCode::KeyD) {
            self.position = self.position + right * velocity;
        }

        if input.is_held(KeyCode::KeyA) {
            self.position = self.position - right * velocity;
        }
    }
}
