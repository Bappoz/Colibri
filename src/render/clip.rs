//! Frustum culling and near-plane clipping, in homogeneous clip space.
//!
//! # Why clip before the perspective divide
//!
//! Dividing by `w` is what turns clip space into normalized device
//! coordinates, and it is undefined at `w = 0` and *sign-flipping* for
//! `w < 0` — geometry behind the camera comes back mirrored through the
//! origin and smears across the screen. Cutting the triangle against the near
//! plane while it is still linear in clip space is the only way to keep the
//! divide well defined.
//!
//! # Why only the near plane is clipped
//!
//! Near is the plane that *must* be clipped, for the reason above. The other
//! five are handled more cheaply elsewhere: a triangle entirely outside one of
//! them is rejected here by [`trivial_reject`] (a few comparisons, no new
//! vertices), and whatever survives is clamped for free by the rasterizer's
//! bounding box. Clipping them for real would allocate vertices that the
//! bounding box discards anyway.

use crate::math::Vec4d;

/// Bit set marking which frustum planes a clip-space point falls outside of.
///
/// One bit per plane, so an entire triangle can be rejected with a single
/// bitwise AND of its three vertices.
pub type Outcode = u8;

/// Outside the near plane (`z < -w`) — behind the camera or too close.
pub const OUT_NEAR: Outcode = 1 << 0;
/// Outside the far plane (`z > w`).
pub const OUT_FAR: Outcode = 1 << 1;
/// Outside the left plane (`x < -w`).
pub const OUT_LEFT: Outcode = 1 << 2;
/// Outside the right plane (`x > w`).
pub const OUT_RIGHT: Outcode = 1 << 3;
/// Outside the bottom plane (`y < -w`).
pub const OUT_BOTTOM: Outcode = 1 << 4;
/// Outside the top plane (`y > w`).
pub const OUT_TOP: Outcode = 1 << 5;

/// A vertex in clip space, i.e. after the model-view-projection matrix and
/// **before** the perspective divide.
///
/// Every attribute is interpolated linearly here, which is exactly why
/// clipping happens at this stage: linearity in clip space is what the
/// `1/w` correction later restores on screen.
#[derive(Clone, Copy, Debug)]
pub struct ClipVertex {
    /// Homogeneous position; `w` is the view-space depth.
    pub pos: Vec4d,
    /// Texture coordinates.
    pub uv: [f64; 2],
    /// Diffuse light intensity for this vertex, in `[0, 1]`.
    pub intensity: f64,
}

impl ClipVertex {
    /// An all-zero vertex, used to prime fixed-size scratch buffers.
    pub const ZERO: Self = Self {
        pos: Vec4d::new(0.0, 0.0, 0.0, 0.0),
        uv: [0.0, 0.0],
        intensity: 0.0,
    };

    /// Signed distance to the near plane in clip space.
    ///
    /// Positive (or zero) means the vertex is on the visible side. The plane
    /// equation is `z + w = 0`, which is the near plane precisely because
    /// [`crate::math::Mat4x4::perspective`] maps `z_view = -near` to
    /// `z_clip = -w_clip`.
    #[inline]
    pub fn near_distance(&self) -> f64 {
        self.pos.z() + self.pos.w()
    }
}

/// Up to two triangles produced by clipping one triangle.
///
/// Fixed-size on purpose: the previous version returned a `Vec`, which meant a
/// heap allocation for every triangle of every mesh of every frame.
#[derive(Clone, Copy, Debug)]
pub struct ClipOutput {
    /// Storage for the results; only the first `count` entries are meaningful.
    triangles: [[ClipVertex; 3]; 2],
    /// How many triangles came out: 0, 1 or 2.
    count: usize,
}

impl ClipOutput {
    /// An empty result — the triangle was entirely clipped away.
    pub const EMPTY: Self = Self {
        triangles: [[ClipVertex::ZERO; 3]; 2],
        count: 0,
    };

    /// The triangles that survived clipping.
    #[inline]
    pub fn as_slice(&self) -> &[[ClipVertex; 3]] {
        &self.triangles[..self.count]
    }

    /// How many triangles survived.
    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether nothing survived.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Computes which frustum planes a clip-space position lies outside of.
#[inline]
pub fn outcode(pos: &Vec4d) -> Outcode {
    let (x, y, z, w) = (pos.x(), pos.y(), pos.z(), pos.w());
    let mut code = 0;
    if z < -w {
        code |= OUT_NEAR;
    }
    if z > w {
        code |= OUT_FAR;
    }
    if x < -w {
        code |= OUT_LEFT;
    }
    if x > w {
        code |= OUT_RIGHT;
    }
    if y < -w {
        code |= OUT_BOTTOM;
    }
    if y > w {
        code |= OUT_TOP;
    }
    code
}

/// Whether the whole triangle sits outside a single frustum plane and can be
/// discarded without clipping anything.
///
/// # Correctness
///
/// The "all vertices outside the same plane ⇒ the triangle is outside" rule
/// relies on the triangle being the convex hull of its vertices. That holds in
/// homogeneous space only while every `w` is positive; with a vertex behind
/// the camera the segment between two vertices wraps through infinity instead
/// of staying between them. So the side planes are only trusted when all three
/// `w` are positive — mixed-sign triangles fall through to the near clip,
/// which splits them into pieces that do satisfy the precondition.
#[inline]
pub fn trivial_reject(a: &ClipVertex, b: &ClipVertex, c: &ClipVertex) -> bool {
    let shared = outcode(&a.pos) & outcode(&b.pos) & outcode(&c.pos);
    if shared == 0 {
        return false;
    }

    let all_in_front = a.pos.w() > 0.0 && b.pos.w() > 0.0 && c.pos.w() > 0.0;
    if all_in_front {
        true
    } else {
        // Only the near plane keeps its meaning for a mixed-sign triangle.
        shared & OUT_NEAR != 0
    }
}

/// Clips a triangle against the near plane, returning 0, 1 or 2 triangles.
///
/// Implemented as one Sutherland-Hodgman pass over the triangle's edges: walk
/// the three edges in order, emit every vertex that is inside, and emit an
/// intersection whenever an edge crosses the plane. Walking the edges in order
/// is what preserves the winding, which back-face culling depends on.
///
/// The resulting polygon has at most 4 vertices, fan-triangulated around the
/// first one.
pub fn clip_near(triangle: [ClipVertex; 3]) -> ClipOutput {
    // 3 original vertices, minus at least one clipped away, plus at most 2
    // intersections — never more than 4.
    let mut polygon = [ClipVertex::ZERO; 4];
    let mut count = 0;

    for i in 0..3 {
        let current = triangle[i];
        let next = triangle[(i + 1) % 3];
        let d_current = current.near_distance();
        let d_next = next.near_distance();

        if d_current >= 0.0 {
            polygon[count] = current;
            count += 1;
        }
        // Sign change means this edge crosses the plane.
        if (d_current >= 0.0) != (d_next >= 0.0) {
            polygon[count] = intersect_near(&current, &next, d_current, d_next);
            count += 1;
        }
    }

    match count {
        3 => ClipOutput {
            triangles: [[polygon[0], polygon[1], polygon[2]], [ClipVertex::ZERO; 3]],
            count: 1,
        },
        4 => ClipOutput {
            triangles: [
                [polygon[0], polygon[1], polygon[2]],
                [polygon[0], polygon[2], polygon[3]],
            ],
            count: 2,
        },
        // 0 (fully clipped) or a degenerate 1-2 vertex sliver: nothing to draw.
        _ => ClipOutput::EMPTY,
    }
}

/// Splits the edge `from -> to` at the near plane, interpolating every
/// attribute with the same parameter as the position.
///
/// `d_from` and `d_to` are the signed distances already computed by the
/// caller, so the plane equation is not evaluated twice.
fn intersect_near(from: &ClipVertex, to: &ClipVertex, d_from: f64, d_to: f64) -> ClipVertex {
    let denominator = d_from - d_to;
    // The caller only reaches here on a sign change, so the denominator is
    // non-zero except for degenerate coordinates; falling back to the `from`
    // endpoint keeps the output finite instead of NaN.
    let t = if denominator.abs() > f64::EPSILON {
        d_from / denominator
    } else {
        0.0
    };

    let lerp = |a: f64, b: f64| a + t * (b - a);

    ClipVertex {
        pos: Vec4d::new(
            lerp(from.pos.x(), to.pos.x()),
            lerp(from.pos.y(), to.pos.y()),
            lerp(from.pos.z(), to.pos.z()),
            lerp(from.pos.w(), to.pos.w()),
        ),
        uv: [lerp(from.uv[0], to.uv[0]), lerp(from.uv[1], to.uv[1])],
        intensity: lerp(from.intensity, to.intensity),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vértice em clip space com atributos triviais, para encurtar os testes.
    fn vertex(x: f64, y: f64, z: f64, w: f64) -> ClipVertex {
        ClipVertex {
            pos: Vec4d::new(x, y, z, w),
            uv: [0.0, 0.0],
            intensity: 1.0,
        }
    }

    /// Triângulo inteiramente visível passa sem ser tocado.
    #[test]
    fn fully_inside_triangle_passes_through() {
        let tri = [
            vertex(0.0, 0.0, 0.0, 1.0),
            vertex(0.5, 0.0, 0.0, 1.0),
            vertex(0.0, 0.5, 0.0, 1.0),
        ];
        let out = clip_near(tri);
        assert_eq!(out.len(), 1);
        assert_eq!(out.as_slice()[0][0].pos, tri[0].pos);
    }

    /// Triângulo inteiramente atrás do near desaparece.
    #[test]
    fn fully_behind_triangle_is_discarded() {
        let tri = [
            vertex(0.0, 0.0, -5.0, 1.0),
            vertex(1.0, 0.0, -5.0, 1.0),
            vertex(0.0, 1.0, -5.0, 1.0),
        ];
        assert!(clip_near(tri).is_empty());
    }

    /// Um vértice fora gera um quad, que sai fatiado em dois triângulos.
    #[test]
    fn one_vertex_behind_produces_two_triangles() {
        let tri = [
            vertex(0.0, 0.0, 0.0, 1.0),
            vertex(1.0, 0.0, 0.0, 1.0),
            vertex(0.0, 1.0, -3.0, 1.0), // z < -w
        ];
        let out = clip_near(tri);
        assert_eq!(out.len(), 2);
        for triangle in out.as_slice() {
            for v in triangle {
                assert!(v.near_distance() >= -1e-9, "sobrou vértice atrás do near");
            }
        }
    }

    /// Dois vértices fora deixam um único triângulo recortado.
    #[test]
    fn two_vertices_behind_produce_one_triangle() {
        let tri = [
            vertex(0.0, 0.0, 0.0, 1.0),
            vertex(1.0, 0.0, -3.0, 1.0),
            vertex(0.0, 1.0, -3.0, 1.0),
        ];
        let out = clip_near(tri);
        assert_eq!(out.len(), 1);
        for v in &out.as_slice()[0] {
            assert!(v.near_distance() >= -1e-9);
        }
    }

    /// O corte preserva o winding — sem isso o back-face culling apaga o que
    /// deveria aparecer.
    #[test]
    fn clipping_preserves_winding() {
        let signed_area = |t: &[ClipVertex; 3]| {
            let p: Vec<(f64, f64)> = t
                .iter()
                .map(|v| (v.pos.x() / v.pos.w(), v.pos.y() / v.pos.w()))
                .collect();
            (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[1].1 - p[0].1) * (p[2].0 - p[0].0)
        };

        let tri = [
            vertex(0.0, 0.0, 0.0, 1.0),
            vertex(1.0, 0.0, 0.0, 1.0),
            vertex(0.0, 1.0, -3.0, 1.0),
        ];
        let original = signed_area(&tri);
        for clipped in clip_near(tri).as_slice() {
            assert!(
                signed_area(clipped) * original > 0.0,
                "o winding inverteu no clipping"
            );
        }
    }

    /// Atributos são interpolados junto com a posição, sem extrapolar.
    #[test]
    fn attributes_are_interpolated_at_the_cut() {
        let mut tri = [
            vertex(0.0, 0.0, 1.0, 1.0),
            vertex(1.0, 0.0, 1.0, 1.0),
            vertex(0.0, 1.0, -3.0, 1.0),
        ];
        tri[0].uv = [0.0, 0.0];
        tri[1].uv = [1.0, 0.0];
        tri[2].uv = [0.0, 1.0];

        for triangle in clip_near(tri).as_slice() {
            for v in triangle {
                assert!((0.0..=1.0).contains(&v.uv[0]));
                assert!((0.0..=1.0).contains(&v.uv[1]));
            }
        }
    }

    /// Triângulo todo à direita do frustum é descartado sem gerar vértice.
    #[test]
    fn offscreen_triangle_is_trivially_rejected() {
        let a = vertex(5.0, 0.0, 0.0, 1.0);
        let b = vertex(6.0, 0.0, 0.0, 1.0);
        let c = vertex(5.0, 1.0, 0.0, 1.0);
        assert!(trivial_reject(&a, &b, &c));
    }

    /// Um triângulo que cruza a borda não pode ser rejeitado.
    #[test]
    fn straddling_triangle_is_not_rejected() {
        let a = vertex(0.0, 0.0, 0.0, 1.0);
        let b = vertex(5.0, 0.0, 0.0, 1.0);
        let c = vertex(0.0, 1.0, 0.0, 1.0);
        assert!(!trivial_reject(&a, &b, &c));
    }

    /// Com `w` negativo o teste lateral não vale: só o near pode rejeitar.
    #[test]
    fn mixed_sign_w_only_trusts_the_near_plane() {
        let a = vertex(5.0, 0.0, 0.0, 1.0);
        let b = vertex(5.0, 0.0, 0.0, -1.0);
        let c = vertex(5.0, 1.0, 0.0, 1.0);
        assert!(!trivial_reject(&a, &b, &c));

        let behind = [
            vertex(0.0, 0.0, -5.0, 1.0),
            vertex(0.0, 0.0, -5.0, -1.0),
            vertex(0.0, 1.0, -5.0, 1.0),
        ];
        assert!(trivial_reject(&behind[0], &behind[1], &behind[2]));
    }
}
