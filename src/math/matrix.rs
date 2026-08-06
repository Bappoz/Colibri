//! 4x4 transformation matrices.
//!
//! # Conventions (read this before touching a matrix)
//!
//! * **Column vectors.** A transform is applied as `M * v`, so composing
//!   `A * B * v` applies `B` first and `A` last. Read a chain right to left.
//! * **Row-major storage.** [`Mat4x4`] holds the four *rows*; the
//!   translation of a TRS matrix therefore lives in the `w` component of the
//!   first three rows.
//! * **Right-handed, `-Z` forward.** [`Mat4x4::perspective`] follows the
//!   OpenGL convention, so a point in front of the camera has negative `z` in
//!   view space and clip-space `w` ends up positive.
//! * **Counter-clockwise front faces** in the source data; after the
//!   `y`-flip of the viewport transform they become clockwise on screen, which
//!   is what [`crate::render::raster`] assumes when it culls back faces.

use std::ops::Mul;

use super::vec::{Vec3d, Vec4d};

/// A 4x4 matrix in homogeneous coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4x4 {
    /// The four rows of the matrix, top to bottom.
    m: [Vec4d; 4],
}

impl Mat4x4 {
    /// The identity transform.
    pub const fn identity() -> Self {
        Self {
            m: [
                Vec4d::new(1.0, 0.0, 0.0, 0.0),
                Vec4d::new(0.0, 1.0, 0.0, 0.0),
                Vec4d::new(0.0, 0.0, 1.0, 0.0),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Returns row `i` (`0..4`), top to bottom.
    pub const fn row(&self, i: usize) -> Vec4d {
        self.m[i]
    }

    /// Returns column `j` (`0..4`), left to right.
    ///
    /// The first three columns of a rigid transform are its basis vectors
    /// (right, up, back) and the fourth is its translation.
    pub fn col(&self, j: usize) -> Vec4d {
        Vec4d::new(self.m[0][j], self.m[1][j], self.m[2][j], self.m[3][j])
    }

    /// Perspective projection matrix, OpenGL convention (right-handed,
    /// `-Z` forward, clip space normalized to `[-1, 1]` on all three axes).
    ///
    /// * `fov_y_radians` — vertical field of view.
    /// * `aspect` — viewport width divided by height.
    /// * `near` / `far` — positive distances to the clipping planes.
    ///
    /// The bottom row is `(0, 0, -1, 0)`, so `w_clip = -z_view`: everything in
    /// front of the camera comes out with positive `w`, which is what the near
    /// plane test `z >= -w` in [`crate::render::clip`] relies on.
    pub fn perspective(fov_y_radians: f64, aspect: f64, near: f64, far: f64) -> Self {
        let f = 1.0 / (fov_y_radians / 2.0).tan();
        Self {
            m: [
                Vec4d::new(f / aspect, 0.0, 0.0, 0.0),
                Vec4d::new(0.0, f, 0.0, 0.0),
                Vec4d::new(
                    0.0,
                    0.0,
                    (far + near) / (near - far),
                    (2.0 * far * near) / (near - far),
                ),
                Vec4d::new(0.0, 0.0, -1.0, 0.0),
            ],
        }
    }

    /// Translation by `t`.
    pub const fn translation(t: Vec3d) -> Self {
        Self {
            m: [
                Vec4d::new(1.0, 0.0, 0.0, t.x()),
                Vec4d::new(0.0, 1.0, 0.0, t.y()),
                Vec4d::new(0.0, 0.0, 1.0, t.z()),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Non-uniform scale by `s`.
    ///
    /// Note that a non-uniform scale invalidates the "rotate the normal with
    /// the model matrix" shortcut used by the renderer; see
    /// [`crate::scene::Transform`] for the trade-off taken there.
    pub const fn scale(s: Vec3d) -> Self {
        Self {
            m: [
                Vec4d::new(s.x(), 0.0, 0.0, 0.0),
                Vec4d::new(0.0, s.y(), 0.0, 0.0),
                Vec4d::new(0.0, 0.0, s.z(), 0.0),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Rotation around the `X` axis (pitch), counter-clockwise looking down
    /// `-X`.
    pub fn rotation_x(radians: f64) -> Self {
        let (s, c) = radians.sin_cos(); // one call computes both
        Self {
            m: [
                Vec4d::new(1.0, 0.0, 0.0, 0.0),
                Vec4d::new(0.0, c, -s, 0.0),
                Vec4d::new(0.0, s, c, 0.0),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Rotation around the `Y` axis (yaw).
    pub fn rotation_y(radians: f64) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            m: [
                Vec4d::new(c, 0.0, s, 0.0),
                Vec4d::new(0.0, 1.0, 0.0, 0.0),
                Vec4d::new(-s, 0.0, c, 0.0),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Rotation around the `Z` axis (roll).
    pub fn rotation_z(radians: f64) -> Self {
        let (s, c) = radians.sin_cos();
        Self {
            m: [
                Vec4d::new(c, -s, 0.0, 0.0),
                Vec4d::new(s, c, 0.0, 0.0),
                Vec4d::new(0.0, 0.0, 1.0, 0.0),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Builds a **camera-to-world** matrix: an object sitting at `pos` looking
    /// at `target`, with `up` as the roll reference.
    ///
    /// The `up` argument only has to be roughly correct — it is
    /// re-orthogonalized against the forward axis (Gram-Schmidt) before the
    /// basis is assembled.
    ///
    /// The third column stores `-forward`, not `forward`: with the OpenGL
    /// convention of [`Mat4x4::perspective`], a point along `forward` must end
    /// up with a negative `z` in view space.
    ///
    /// To get the **world-to-camera** (view) matrix, feed the result to
    /// [`Mat4x4::quick_inverse`].
    pub fn point_at(pos: Vec3d, target: Vec3d, up: Vec3d) -> Self {
        let forward = (target - pos).normalize();
        let projected = forward * up.dot(&forward);
        let up = (up - projected).normalize();

        let right = forward.cross(&up);
        let back = -forward;

        Self {
            m: [
                Vec4d::new(right.x(), up.x(), back.x(), pos.x()),
                Vec4d::new(right.y(), up.y(), back.y(), pos.y()),
                Vec4d::new(right.z(), up.z(), back.z(), pos.z()),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Inverts a rigid transform (orthonormal rotation plus translation) by
    /// transposing the 3x3 block and re-projecting the translation.
    ///
    /// # Correctness
    ///
    /// Only valid when the upper-left 3x3 block is orthonormal — i.e. rotation
    /// and translation, **no scale and no shear**. Feeding it a scaled matrix
    /// silently produces a wrong inverse. It exists because the view matrix is
    /// recomputed every frame and the general 4x4 inverse costs ~10x more.
    pub fn quick_inverse(&self) -> Self {
        let right = Vec3d::new(self.m[0].x(), self.m[1].x(), self.m[2].x());
        let up = Vec3d::new(self.m[0].y(), self.m[1].y(), self.m[2].y());
        let back = Vec3d::new(self.m[0].z(), self.m[1].z(), self.m[2].z());
        let pos = Vec3d::new(self.m[0].w(), self.m[1].w(), self.m[2].w());

        Self {
            m: [
                Vec4d::new(right.x(), right.y(), right.z(), -right.dot(&pos)),
                Vec4d::new(up.x(), up.y(), up.z(), -up.dot(&pos)),
                Vec4d::new(back.x(), back.y(), back.z(), -back.dot(&pos)),
                Vec4d::new(0.0, 0.0, 0.0, 1.0),
            ],
        }
    }

    /// Transforms a **point**: promotes it with `w = 1` so the translation
    /// applies, and returns the raw homogeneous result without dividing by `w`
    /// (the divide happens after clipping).
    pub fn transform_point(&self, p: Vec3d) -> Vec4d {
        *self * Vec4d::from_vec3(p, 1.0)
    }

    /// Transforms a **direction**: promotes it with `w = 0` so the translation
    /// cancels out, and drops `w` from the result.
    ///
    /// For normals this is only correct when the matrix has no non-uniform
    /// scale; otherwise the inverse-transpose would be required.
    pub fn transform_direction(&self, d: Vec3d) -> Vec3d {
        (*self * Vec4d::from_vec3(d, 0.0)).xyz()
    }
}

impl Default for Mat4x4 {
    /// The identity transform.
    fn default() -> Self {
        Self::identity()
    }
}

impl Mul<Vec4d> for Mat4x4 {
    type Output = Vec4d;

    /// Matrix-vector product `M * v` (column-vector convention): each output
    /// component is the dot product of one row with `v`.
    fn mul(self, v: Vec4d) -> Vec4d {
        Vec4d::new(
            self.m[0].dot(&v),
            self.m[1].dot(&v),
            self.m[2].dot(&v),
            self.m[3].dot(&v),
        )
    }
}

impl Mul<Mat4x4> for Mat4x4 {
    type Output = Mat4x4;

    /// Matrix-matrix product. `a * b` applies `b` first, then `a`.
    fn mul(self, rhs: Mat4x4) -> Mat4x4 {
        // The columns of `rhs` are extracted once instead of once per output
        // element: the naive version recomputes them 4x.
        let cols = [rhs.col(0), rhs.col(1), rhs.col(2), rhs.col(3)];
        let mut m = [Vec4d::new(0.0, 0.0, 0.0, 0.0); 4];
        for (out, row) in m.iter_mut().zip(self.m.iter()) {
            *out = Vec4d::new(
                row.dot(&cols[0]),
                row.dot(&cols[1]),
                row.dot(&cols[2]),
                row.dot(&cols[3]),
            );
        }
        Mat4x4 { m }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tolerância usada nas comparações de ponto flutuante dos testes.
    const TOL: f64 = 1e-9;

    fn assert_close(a: Vec4d, b: Vec4d) {
        for i in 0..4 {
            assert!((a[i] - b[i]).abs() < TOL, "esperado {b:?}, obtido {a:?}");
        }
    }

    /// A identidade não mexe em nada, e a multiplicação com ela é neutra.
    #[test]
    fn identity_is_neutral() {
        let m = Mat4x4::translation(Vec3d::new(1.0, 2.0, 3.0));
        assert_eq!(m * Mat4x4::identity(), m);
        assert_eq!(Mat4x4::identity() * m, m);
    }

    /// `A * B` aplica B primeiro: escalar e depois transladar != o inverso.
    #[test]
    fn multiplication_applies_right_to_left() {
        let t = Mat4x4::translation(Vec3d::new(10.0, 0.0, 0.0));
        let s = Mat4x4::scale(Vec3d::splat(2.0));
        let p = Vec3d::new(1.0, 0.0, 0.0);

        assert_close((t * s).transform_point(p), Vec4d::new(12.0, 0.0, 0.0, 1.0));
        assert_close((s * t).transform_point(p), Vec4d::new(22.0, 0.0, 0.0, 1.0));
    }

    /// `w = 0` cancela a translação — é o que mantém normais corretas.
    #[test]
    fn directions_ignore_translation() {
        let m = Mat4x4::translation(Vec3d::new(5.0, 5.0, 5.0));
        assert_eq!(m.transform_direction(Vec3d::UP), Vec3d::UP);
    }

    /// A matriz de view desfaz exatamente a camera-to-world.
    #[test]
    fn quick_inverse_undoes_point_at() {
        let pos = Vec3d::new(2.0, 3.0, 4.0);
        let camera_to_world = Mat4x4::point_at(pos, Vec3d::ZERO, Vec3d::UP);
        let round_trip = camera_to_world.quick_inverse() * camera_to_world;
        for i in 0..4 {
            assert_close(round_trip.row(i), Mat4x4::identity().row(i));
        }
    }

    /// Com a convenção OpenGL, o que está à frente da câmera vai para `-z`
    /// em view space e sai com `w > 0` do clip space.
    #[test]
    fn forward_maps_to_negative_view_z() {
        let view =
            Mat4x4::point_at(Vec3d::ZERO, Vec3d::new(0.0, 0.0, -1.0), Vec3d::UP).quick_inverse();
        let in_front = view.transform_point(Vec3d::new(0.0, 0.0, -5.0));
        assert!(in_front.z() < 0.0);

        let clip = Mat4x4::perspective(90.0_f64.to_radians(), 1.0, 0.1, 100.0) * in_front;
        assert!(clip.w() > 0.0);
    }

    /// O near plane vira `z = -w` e o far plane vira `z = +w` no clip space.
    #[test]
    fn perspective_maps_near_and_far_to_clip_bounds() {
        let proj = Mat4x4::perspective(90.0_f64.to_radians(), 1.0, 1.0, 100.0);

        let near = proj.transform_point(Vec3d::new(0.0, 0.0, -1.0));
        assert!((near.z() / near.w() + 1.0).abs() < TOL);

        let far = proj.transform_point(Vec3d::new(0.0, 0.0, -100.0));
        assert!((far.z() / far.w() - 1.0).abs() < TOL);
    }
}
