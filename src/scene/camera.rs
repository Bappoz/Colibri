//! Free-flying first-person camera.

use winit::keyboard::KeyCode;

use crate::engine::InputState;
use crate::math::{Mat4x4, Vec3d};

/// Pitch limit, just under a right angle.
///
/// At exactly 90° the forward vector becomes parallel to the world up axis and
/// the camera basis collapses (the cross product used for `right` degenerates)
/// — the classic gimbal flip. Stopping one degree short avoids it entirely.
const PITCH_LIMIT_RADIANS: f64 = 1.552_913_9; // 89°

/// A camera defined by a position and two Euler angles, plus its lens.
///
/// Roll is deliberately absent: a first-person camera that can tilt sideways
/// is disorienting, and leaving it out keeps the world up axis a valid
/// reference for [`Mat4x4::point_at`].
#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// Position in world space.
    pub position: Vec3d,
    /// Rotation around the world up axis, in radians. Grows counter-clockwise
    /// seen from above.
    pub yaw: f64,
    /// Rotation around the camera's right axis, in radians. Clamped to
    /// ±89°, just short of the gimbal flip.
    pub pitch: f64,
    /// Vertical field of view, in radians.
    pub fov_y: f64,
    /// Distance to the near clip plane. Everything closer is clipped away;
    /// raising it buys depth-buffer precision.
    pub near: f64,
    /// Distance to the far clip plane.
    pub far: f64,
}

/// Default vertical field of view, in radians (60°).
///
/// A wider lens exaggerates perspective near the screen edges; 60° vertical is
/// the usual compromise between "can see enough" and "does not look warped".
const DEFAULT_FOV_Y: f64 = std::f64::consts::FRAC_PI_3;

impl Default for Camera {
    /// A camera at the origin looking down `-Z`, with a 60° vertical FOV.
    fn default() -> Self {
        Self {
            position: Vec3d::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: DEFAULT_FOV_Y,
            near: 0.1,
            far: 1000.0,
        }
    }
}

impl Camera {
    /// A camera at `position`, looking down `-Z`, with default lens settings.
    pub fn new(position: Vec3d) -> Self {
        Self {
            position,
            ..Self::default()
        }
    }

    /// Unit vector the camera is looking along.
    ///
    /// Derived from the two angles rather than stored, so the camera can never
    /// drift out of sync with itself.
    pub fn forward(&self) -> Vec3d {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        Vec3d::new(-sin_yaw * cos_pitch, sin_pitch, -cos_yaw * cos_pitch)
    }

    /// Unit vector pointing to the camera's right.
    ///
    /// Crossed against the *world* up axis, not the camera's own, so
    /// strafing stays horizontal while looking up or down.
    pub fn right(&self) -> Vec3d {
        self.forward().cross(&Vec3d::UP).normalize()
    }

    /// World-to-camera matrix.
    ///
    /// Built as camera-to-world ([`Mat4x4::point_at`]) and inverted, because
    /// that is the direction the two angles naturally describe.
    pub fn view_matrix(&self) -> Mat4x4 {
        let target = self.position + self.forward();
        Mat4x4::point_at(self.position, target, Vec3d::UP).quick_inverse()
    }

    /// Projection matrix for a viewport of the given aspect ratio
    /// (width / height).
    pub fn projection_matrix(&self, aspect: f64) -> Mat4x4 {
        Mat4x4::perspective(self.fov_y, aspect, self.near, self.far)
    }

    /// Applies a raw mouse delta, in pixels, scaled by `sensitivity`.
    ///
    /// `dy` is subtracted because screen coordinates grow downwards while
    /// pitch grows upwards.
    pub fn process_mouse(&mut self, dx: f64, dy: f64, sensitivity: f64) {
        self.yaw += dx * sensitivity;
        self.pitch =
            (self.pitch - dy * sensitivity).clamp(-PITCH_LIMIT_RADIANS, PITCH_LIMIT_RADIANS);
    }

    /// Moves the camera from the currently held keys.
    ///
    /// * `W`/`S` — along the view direction (flying, not walking: looking up
    ///   and pressing `W` gains altitude).
    /// * `A`/`D` — strafe, always horizontal.
    /// * `Space`/`Left Ctrl` — straight up and down in world space.
    /// * `Left Shift` — sprint multiplier.
    ///
    /// Displacement is scaled by `dt`, so speed is independent of frame rate.
    pub fn process_keyboard(&mut self, input: &InputState, speed: f64, dt: f64) {
        let sprint = if input.is_held(KeyCode::ShiftLeft) {
            3.0
        } else {
            1.0
        };
        let distance = speed * sprint * dt;
        let (forward, right) = (self.forward(), self.right());

        if input.is_held(KeyCode::KeyW) {
            self.position += forward * distance;
        }
        if input.is_held(KeyCode::KeyS) {
            self.position -= forward * distance;
        }
        if input.is_held(KeyCode::KeyD) {
            self.position += right * distance;
        }
        if input.is_held(KeyCode::KeyA) {
            self.position -= right * distance;
        }
        if input.is_held(KeyCode::Space) {
            self.position += Vec3d::UP * distance;
        }
        if input.is_held(KeyCode::ControlLeft) {
            self.position -= Vec3d::UP * distance;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sem rotação, a câmera olha para `-Z` e o "direita" dela é `+X`.
    #[test]
    fn default_orientation_looks_down_negative_z() {
        let camera = Camera::default();
        let forward = camera.forward();
        assert!((forward.z() + 1.0).abs() < 1e-12);
        assert!((camera.right().x() - 1.0).abs() < 1e-12);
    }

    /// O pitch trava antes dos 90° — é o que evita o gimbal flip.
    #[test]
    fn pitch_is_clamped() {
        let mut camera = Camera::default();
        camera.process_mouse(0.0, -10_000.0, 0.01);
        assert!((camera.pitch - PITCH_LIMIT_RADIANS).abs() < 1e-12);

        camera.process_mouse(0.0, 10_000.0, 0.01);
        assert!((camera.pitch + PITCH_LIMIT_RADIANS).abs() < 1e-12);
    }

    /// Olhando para cima ou para baixo, o strafe continua horizontal.
    #[test]
    fn strafing_stays_horizontal_while_looking_up() {
        let camera = Camera {
            pitch: 1.0,
            ..Camera::default()
        };
        assert!(camera.right().y().abs() < 1e-12);
    }

    /// A view leva a posição da câmera para a origem do view space.
    #[test]
    fn view_matrix_puts_the_camera_at_the_origin() {
        let camera = Camera::new(Vec3d::new(3.0, -2.0, 7.0));
        let origin = camera.view_matrix().transform_point(camera.position);
        assert!(origin.xyz().length() < 1e-9);
    }
}
