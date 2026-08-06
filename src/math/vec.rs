//! Vector primitives (`Vec3d`, `Vec4d`).
//!
//! Colibri implements its own linear algebra on purpose: understanding the
//! mechanism *is* the goal of the project, so `glam`/`nalgebra` are not used.
//!
//! # Conventions
//!
//! * Right-handed world space, `+Y` up, `-Z` forward (OpenGL style).
//! * Vectors are **column vectors**: a transform is applied as `M * v`.
//! * All components are `f64`. The software rasterizer is precision-bound long
//!   before it is bandwidth-bound, so the wider type is the cheaper trade.

use std::ops::{Add, AddAssign, Index, Mul, Neg, Sub, SubAssign};

/// Length below which a vector is considered degenerate and cannot be
/// normalized in a numerically meaningful way.
const EPSILON: f64 = 1e-12;

// ===========================================================================
//                                  Vec3d
// ===========================================================================

/// A 3-component vector, used for positions, directions, scales and colors.
///
/// The components are kept in a private array so the type can only be built
/// through [`Vec3d::new`] and read through the accessors — that keeps the
/// storage layout an implementation detail.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec3d {
    /// Components in `[x, y, z]` order.
    v: [f64; 3],
}

impl Vec3d {
    /// The zero vector — also the fallback returned by [`Vec3d::normalize`]
    /// for degenerate input.
    pub const ZERO: Self = Self { v: [0.0, 0.0, 0.0] };
    /// World up axis (`+Y`), the reference used to build camera bases.
    pub const UP: Self = Self { v: [0.0, 1.0, 0.0] };
    /// World right axis (`+X`).
    pub const RIGHT: Self = Self { v: [1.0, 0.0, 0.0] };
    /// World forward axis (`-Z`), matching the OpenGL-style convention used by
    /// [`crate::math::Mat4x4::perspective`].
    pub const FORWARD: Self = Self {
        v: [0.0, 0.0, -1.0],
    };

    /// Builds a vector from its three components.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { v: [x, y, z] }
    }

    /// Builds a vector with the same value on every component.
    pub const fn splat(value: f64) -> Self {
        Self::new(value, value, value)
    }

    /// The `x` component.
    pub const fn x(&self) -> f64 {
        self.v[0]
    }

    /// The `y` component.
    pub const fn y(&self) -> f64 {
        self.v[1]
    }

    /// The `z` component.
    pub const fn z(&self) -> f64 {
        self.v[2]
    }

    /// Dot product. For unit vectors this is the cosine of the angle between
    /// them — the basis of the diffuse lighting term.
    pub fn dot(&self, other: &Vec3d) -> f64 {
        self.x() * other.x() + self.y() * other.y() + self.z() * other.z()
    }

    /// Cross product, following the right-hand rule: `a.cross(b)` is
    /// perpendicular to both and points along the thumb when the fingers curl
    /// from `a` to `b`.
    pub fn cross(&self, other: &Vec3d) -> Vec3d {
        Vec3d::new(
            self.y() * other.z() - self.z() * other.y(),
            self.z() * other.x() - self.x() * other.z(),
            self.x() * other.y() - self.y() * other.x(),
        )
    }

    /// Squared length. Prefer this over [`Vec3d::length`] for comparisons — it
    /// skips the square root.
    pub fn length_squared(&self) -> f64 {
        self.dot(self)
    }

    /// Euclidean length.
    pub fn length(&self) -> f64 {
        self.length_squared().sqrt()
    }

    /// Returns a unit-length copy, or [`Vec3d::ZERO`] when the vector is too
    /// short to have a meaningful direction.
    ///
    /// Degenerate input is real in a renderer (zero-area faces produce zero
    /// normals), so this returns a defined value instead of `NaN`. Use
    /// [`Vec3d::try_normalize`] when the caller needs to detect that case.
    pub fn normalize(&self) -> Vec3d {
        self.try_normalize().unwrap_or(Vec3d::ZERO)
    }

    /// Returns a unit-length copy, or `None` when the vector is degenerate.
    pub fn try_normalize(&self) -> Option<Vec3d> {
        let length = self.length();
        (length > EPSILON).then(|| *self * (1.0 / length))
    }

    /// Component-wise linear interpolation; `t = 0` yields `self`, `t = 1`
    /// yields `other`.
    pub fn lerp(&self, other: &Vec3d, t: f64) -> Vec3d {
        *self + (*other - *self) * t
    }
}

impl Add for Vec3d {
    type Output = Vec3d;

    /// Component-wise addition.
    fn add(self, other: Vec3d) -> Vec3d {
        Vec3d::new(
            self.x() + other.x(),
            self.y() + other.y(),
            self.z() + other.z(),
        )
    }
}

impl AddAssign for Vec3d {
    /// Component-wise in-place addition.
    fn add_assign(&mut self, other: Vec3d) {
        *self = *self + other;
    }
}

impl Sub for Vec3d {
    type Output = Vec3d;

    /// Component-wise subtraction.
    fn sub(self, other: Vec3d) -> Vec3d {
        Vec3d::new(
            self.x() - other.x(),
            self.y() - other.y(),
            self.z() - other.z(),
        )
    }
}

impl SubAssign for Vec3d {
    /// Component-wise in-place subtraction.
    fn sub_assign(&mut self, other: Vec3d) {
        *self = *self - other;
    }
}

impl Mul<f64> for Vec3d {
    type Output = Vec3d;

    /// Uniform scaling by a scalar.
    fn mul(self, scalar: f64) -> Vec3d {
        Vec3d::new(self.x() * scalar, self.y() * scalar, self.z() * scalar)
    }
}

impl Neg for Vec3d {
    type Output = Vec3d;

    /// Reverses the direction of the vector.
    fn neg(self) -> Vec3d {
        Vec3d::new(-self.x(), -self.y(), -self.z())
    }
}

impl Index<usize> for Vec3d {
    type Output = f64;

    /// Component access by index (`0 = x`, `1 = y`, `2 = z`).
    fn index(&self, i: usize) -> &f64 {
        &self.v[i]
    }
}

// ===========================================================================
//                                  Vec4d
// ===========================================================================

/// A homogeneous 4-component vector.
///
/// Points carry `w = 1` so translations apply to them; directions carry
/// `w = 0` so translations cancel out. After the projection matrix, `w` holds
/// the view-space depth used for the perspective divide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec4d {
    /// Components in `[x, y, z, w]` order.
    v: [f64; 4],
}

impl Vec4d {
    /// Builds a vector from its four components.
    pub const fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { v: [x, y, z, w] }
    }

    /// Promotes a [`Vec3d`] to homogeneous space with an explicit `w`
    /// (`1.0` for a point, `0.0` for a direction).
    pub const fn from_vec3(v: Vec3d, w: f64) -> Self {
        Self::new(v.v[0], v.v[1], v.v[2], w)
    }

    /// The `x` component.
    pub const fn x(&self) -> f64 {
        self.v[0]
    }

    /// The `y` component.
    pub const fn y(&self) -> f64 {
        self.v[1]
    }

    /// The `z` component.
    pub const fn z(&self) -> f64 {
        self.v[2]
    }

    /// The `w` (homogeneous) component.
    pub const fn w(&self) -> f64 {
        self.v[3]
    }

    /// Drops `w`, keeping the `xyz` part unchanged (no perspective divide).
    pub const fn xyz(&self) -> Vec3d {
        Vec3d::new(self.v[0], self.v[1], self.v[2])
    }

    /// Four-component dot product — the building block of matrix products.
    pub fn dot(&self, other: &Vec4d) -> f64 {
        self.x() * other.x() + self.y() * other.y() + self.z() * other.z() + self.w() * other.w()
    }
}

impl Index<usize> for Vec4d {
    type Output = f64;

    /// Component access by index (`0 = x`, `1 = y`, `2 = z`, `3 = w`).
    fn index(&self, i: usize) -> &f64 {
        &self.v[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O produto vetorial dos eixos canônicos segue a regra da mão direita.
    #[test]
    fn cross_follows_right_hand_rule() {
        let z = Vec3d::RIGHT.cross(&Vec3d::UP);
        assert_eq!(z, Vec3d::new(0.0, 0.0, 1.0));
    }

    /// Normalizar preserva a direção e leva o comprimento a 1.
    #[test]
    fn normalize_yields_unit_length() {
        let n = Vec3d::new(3.0, 0.0, 4.0).normalize();
        assert!((n.length() - 1.0).abs() < 1e-12);
        assert!((n.x() - 0.6).abs() < 1e-12);
    }

    /// Vetor degenerado não pode virar `NaN`: vira `ZERO` e sinaliza via `try_`.
    #[test]
    fn normalize_of_zero_is_defined() {
        assert_eq!(Vec3d::ZERO.normalize(), Vec3d::ZERO);
        assert!(Vec3d::ZERO.try_normalize().is_none());
    }

    /// `from_vec3` com `w = 0` marca direção; `xyz` desfaz a promoção.
    #[test]
    fn homogeneous_round_trip() {
        let v = Vec3d::new(1.0, 2.0, 3.0);
        assert_eq!(Vec4d::from_vec3(v, 0.0).xyz(), v);
        assert_eq!(Vec4d::from_vec3(v, 1.0).w(), 1.0);
    }
}
