//! Position, orientation and scale of an object in world space.

use crate::math::{Mat4x4, Vec3d};

/// A TRS transform: translation, rotation and scale.
///
/// Rotation is stored as Euler angles in radians. Euler angles are ambiguous
/// and gimbal-lock, but they are readable in a debugger and enough for a scene
/// of independent objects; quaternions land with the animation stage, when
/// interpolating orientations actually matters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Position in world space.
    pub translation: Vec3d,
    /// Euler angles in radians, applied as yaw (`y`), then pitch (`x`), then
    /// roll (`z`).
    pub rotation: Vec3d,
    /// Per-axis scale. Keep it uniform unless you also fix the normal
    /// transform — see [`Transform::matrix`].
    pub scale: Vec3d,
}

impl Default for Transform {
    /// The identity transform: at the origin, unrotated, unit scale.
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// At the origin, unrotated, unit scale.
    pub const IDENTITY: Self = Self {
        translation: Vec3d::ZERO,
        rotation: Vec3d::ZERO,
        scale: Vec3d::splat(1.0),
    };

    /// A transform that only moves the object.
    pub const fn from_translation(translation: Vec3d) -> Self {
        Self {
            translation,
            rotation: Vec3d::ZERO,
            scale: Vec3d::splat(1.0),
        }
    }

    /// Builder-style uniform scale.
    pub const fn with_uniform_scale(mut self, scale: f64) -> Self {
        self.scale = Vec3d::splat(scale);
        self
    }

    /// Builder-style Euler rotation, in radians.
    pub const fn with_rotation(mut self, rotation: Vec3d) -> Self {
        self.rotation = rotation;
        self
    }

    /// Builds the model matrix, `T * Ry * Rx * Rz * S`.
    ///
    /// Read it right to left: scale first (so scaling happens in the object's
    /// own axes), then roll/pitch/yaw, then the move into world space.
    ///
    /// # Normals
    ///
    /// The renderer transforms normals with this same matrix, which is only
    /// correct while the scale is uniform. Non-uniform scale would require the
    /// inverse transpose of the upper 3x3 block; the cheap path is kept until
    /// a scene actually needs squashed objects.
    pub fn matrix(&self) -> Mat4x4 {
        Mat4x4::translation(self.translation)
            * Mat4x4::rotation_y(self.rotation.y())
            * Mat4x4::rotation_x(self.rotation.x())
            * Mat4x4::rotation_z(self.rotation.z())
            * Mat4x4::scale(self.scale)
    }

    /// Whether the scale is the same on all three axes — the precondition for
    /// the normal shortcut described in [`Transform::matrix`].
    pub fn has_uniform_scale(&self) -> bool {
        const TOL: f64 = 1e-9;
        (self.scale.x() - self.scale.y()).abs() < TOL
            && (self.scale.y() - self.scale.z()).abs() < TOL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A identidade não move, não gira e não escala.
    #[test]
    fn identity_leaves_a_point_alone() {
        let p = Vec3d::new(1.0, 2.0, 3.0);
        let out = Transform::IDENTITY.matrix().transform_point(p);
        assert_eq!(out.xyz(), p);
    }

    /// Escala acontece antes da translação — o objeto não é arrastado junto.
    #[test]
    fn scale_applies_before_translation() {
        let t = Transform::from_translation(Vec3d::new(10.0, 0.0, 0.0)).with_uniform_scale(2.0);
        let out = t.matrix().transform_point(Vec3d::new(1.0, 0.0, 0.0));
        assert!((out.x() - 12.0).abs() < 1e-9);
    }

    /// Meia volta em Y inverte X e Z e deixa Y intacto.
    #[test]
    fn yaw_of_pi_flips_x_and_z() {
        let t = Transform::IDENTITY.with_rotation(Vec3d::new(0.0, std::f64::consts::PI, 0.0));
        let out = t.matrix().transform_point(Vec3d::new(1.0, 5.0, 0.0));
        assert!((out.x() + 1.0).abs() < 1e-9);
        assert!((out.y() - 5.0).abs() < 1e-9);
    }

    /// A checagem de escala uniforme protege o atalho da normal.
    #[test]
    fn uniform_scale_is_detected() {
        assert!(Transform::IDENTITY.has_uniform_scale());
        let mut squashed = Transform::IDENTITY;
        squashed.scale = Vec3d::new(1.0, 0.5, 1.0);
        assert!(!squashed.has_uniform_scale());
    }
}
