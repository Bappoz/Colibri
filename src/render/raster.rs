//! Triangle rasterization: the inner loop of the whole engine.
//!
//! # Method
//!
//! Half-space rasterization. For each edge of the triangle there is an *edge
//! function* `e(p)` whose sign says which side of the edge `p` falls on; a
//! pixel is covered when all three agree with the sign of the triangle's
//! area. Dividing each edge function by that area yields the barycentric
//! coordinates, which interpolate every vertex attribute for free.
//!
//! # Why it is written this way
//!
//! Everything interpolated across a triangle — the three edge functions, the
//! depth, `1/w` and the attributes — is **affine in screen space**. So none of
//! it is recomputed per pixel: each quantity is evaluated once at the start of
//! a row and then advanced by a constant, one add per pixel. Coverage is
//! tested on the raw edge values, and the only division left in the inner loop
//! is the perspective `1/(1/w)`, paid solely by pixels that pass the depth
//! test.
//!
//! Each row is also narrowed to the interval where the three edges *can* be
//! satisfied, instead of walking the whole bounding box,
//! and colors are combined in integer fixed point — the previous version spent
//! six integer divisions and six float conversions per shaded pixel.
//!
//! # Conventions
//!
//! The viewport transform flips `y` (screen coordinates grow downwards), so a
//! counter-clockwise triangle in model space ends up with a **negative**
//! signed area on screen. That is the definition of "front facing" used by
//! [`crate::render::Renderer`]. [`fill_triangle`] itself is winding-agnostic:
//! it normalizes the winding so that disabling back-face culling still draws.

use crate::assets::Texture;
use crate::render::clip::ClipVertex;
use crate::render::target::RenderTarget;

/// A vertex ready for rasterization: screen-space position plus the
/// attributes already divided by `w`.
///
/// Attributes are stored pre-divided because only `attribute / w` and `1 / w`
/// vary linearly across the screen. Interpolating those and dividing at the
/// end is what makes texturing perspective-correct; interpolating `u` directly
/// makes textures visibly swim on surfaces seen at an angle.
#[derive(Clone, Copy, Debug)]
pub struct ScreenVertex {
    /// Horizontal pixel coordinate, `0` at the left edge.
    pub x: f64,
    /// Vertical pixel coordinate, `0` at the **top** edge.
    pub y: f64,
    /// Normalized device depth in `[-1, 1]`, `-1` at the near plane.
    ///
    /// Not divided by `w`: after the perspective divide this quantity is
    /// already affine in screen space, so it interpolates directly.
    pub z: f64,
    /// `1 / w_clip`, the reference the other attributes are divided by.
    pub inv_w: f64,
    /// `u / w_clip`.
    pub u_over_w: f64,
    /// `v / w_clip`.
    pub v_over_w: f64,
    /// `intensity / w_clip`.
    pub intensity_over_w: f64,
}

impl ScreenVertex {
    /// Performs the perspective divide and the viewport transform.
    ///
    /// Clip space is `[-1, 1]` on both axes with `y` up; the viewport is
    /// `[0, width] x [0, height]` with `y` down, hence the `1.0 - y` flip.
    ///
    /// # Correctness
    ///
    /// Requires `w > 0`, which is why near-plane clipping
    /// ([`crate::render::clip::clip_near`]) must run first.
    #[inline]
    pub fn from_clip(v: &ClipVertex, width: f64, height: f64) -> Self {
        let inv_w = 1.0 / v.pos.w();
        Self {
            x: (v.pos.x() * inv_w + 1.0) * 0.5 * width,
            y: (1.0 - v.pos.y() * inv_w) * 0.5 * height,
            z: v.pos.z() * inv_w,
            inv_w,
            u_over_w: v.uv[0] * inv_w,
            v_over_w: v.uv[1] * inv_w,
            intensity_over_w: v.intensity * inv_w,
        }
    }
}

/// Edge function: twice the signed area of the triangle `(a, b, p)`.
///
/// Negative on one side of the line `a -> b`, positive on the other, zero on
/// it. Everything else in this module is built on that sign.
#[inline]
fn edge(a: &ScreenVertex, b: &ScreenVertex, px: f64, py: f64) -> f64 {
    (b.x - a.x) * (py - a.y) - (b.y - a.y) * (px - a.x)
}

/// Twice the signed area of the triangle in screen space.
///
/// Negative means front facing under this engine's conventions; see the module
/// documentation.
#[inline]
pub fn signed_area(v: &[ScreenVertex; 3]) -> f64 {
    edge(&v[0], &v[1], v[2].x, v[2].y)
}

/// The tint that leaves a texel untouched; also the marker for the fast path
/// that skips tinting altogether.
const WHITE: u32 = 0x00FF_FFFF;

/// Fixed-point scale where `ONE` means "unchanged".
///
/// A power of two so that the normalization after a multiply is a shift and
/// not a division — the reason the shading path works in `0..=256` rather than
/// the `0..=255` the channels themselves use.
const FIXED_ONE: u32 = 256;

/// Red and blue: the two channels that sit far enough apart in a `0x00RRGGBB`
/// word to be multiplied at the same time without their products colliding.
const LANE_RB: u32 = 0x00FF_00FF;

/// Green, multiplied in the second pass.
const LANE_G: u32 = 0x0000_FF00;

/// Converts a light intensity in `[0, 1]` to the fixed point the shading path
/// works in. Values outside the range are clamped; `NaN` maps to zero.
#[inline]
fn intensity_to_fixed(intensity: f64) -> u32 {
    (intensity.clamp(0.0, 1.0) * FIXED_ONE as f64) as u32
}

/// Multiplies a packed `0x00RRGGBB` color by a `0..=256` fixed-point scalar.
///
/// The three channels are done in two multiplies instead of three, by relying
/// on the gaps in the packed layout: red and blue are 16 bits apart, so
/// `0x00RR00BB * 256` still leaves each product inside its own 16-bit lane.
/// Green is handled on its own because it has no such partner.
///
/// This replaced three channel extractions, three multiplies and three
/// divisions by 255 — the divisions alone dominated the previous shading path.
#[inline]
fn scale_packed(color: u32, scale: u32) -> u32 {
    let red_blue = (((color & LANE_RB) * scale) >> 8) & LANE_RB;
    let green = (((color & LANE_G) * scale) >> 8) & LANE_G;
    red_blue | green
}

/// Multiplies two `0x00RRGGBB` colors channel by channel.
///
/// Used to apply a per-object tint on top of the sampled texel without
/// replacing it: modulating by white is a no-op.
#[inline]
pub fn modulate(a: u32, b: u32) -> u32 {
    let channel = |shift: u32| ((((a >> shift) & 0xFF) * ((b >> shift) & 0xFF)) / 255) << shift;
    channel(16) | channel(8) | channel(0)
}

/// Scales a `0x00RRGGBB` color by a light intensity in `[0, 1]`.
#[inline]
pub fn scale_color(color: u32, intensity: f64) -> u32 {
    scale_packed(color, intensity_to_fixed(intensity))
}

/// Applies a per-triangle tint to a texel, in the same fixed point as the
/// light intensity so the two can be folded into one multiply chain.
///
/// Returned as a `0..=256` triple packed the same way a color is, ready to
/// feed [`scale_packed`] twice.
#[inline]
fn tint_to_fixed(tint: u32) -> [u32; 3] {
    // `+ (c >> 7)` turns 255 into 256, so a white tint is exactly neutral
    // instead of losing one part in 256 on every channel.
    let channel = |shift: u32| {
        let c = (tint >> shift) & 0xFF;
        c + (c >> 7)
    };
    [channel(16), channel(8), channel(0)]
}

/// Applies a tint, already in fixed point, to a texel.
///
/// Channel by channel, unlike [`scale_packed`]: the two-lane trick needs one
/// scalar for every lane, and a tint has a different factor per channel.
#[inline]
fn modulate_fixed(texel: u32, tint: [u32; 3]) -> u32 {
    let red = ((((texel >> 16) & 0xFF) * tint[0]) >> 8) & 0xFF;
    let green = ((((texel >> 8) & 0xFF) * tint[1]) >> 8) & 0xFF;
    let blue = (((texel & 0xFF) * tint[2]) >> 8) & 0xFF;
    (red << 16) | (green << 8) | blue
}

/// One quantity interpolated across the triangle, plus its screen-space
/// gradients.
///
/// Any attribute written as `(e0*a0 + e1*a1 + e2*a2) / area` is affine in
/// screen space, so its derivatives are constants and it can be advanced by
/// addition instead of re-evaluated. That is the whole trick: five of these
/// replace twelve multiplications per pixel with five adds.
#[derive(Clone, Copy)]
struct Lerp {
    /// Value at the left edge of the row currently being rasterized.
    row: f64,
    /// Change per one-pixel step in `+x`.
    dx: f64,
    /// Change per one-row step in `+y`.
    dy: f64,
}

impl Lerp {
    /// Builds the interpolator for the attribute holding `values` at the three
    /// vertices, given the edge functions already sampled at the first pixel.
    fn new(
        values: [f64; 3],
        edge_row: &[f64; 3],
        step_x: &[f64; 3],
        step_y: &[f64; 3],
        inv_area: f64,
    ) -> Self {
        let combine =
            |c: &[f64; 3]| (c[0] * values[0] + c[1] * values[1] + c[2] * values[2]) * inv_area;
        Self {
            row: combine(edge_row),
            dx: combine(step_x),
            dy: combine(step_y),
        }
    }

    /// Value `k` pixels into the current row.
    #[inline]
    fn at(&self, k: f64) -> f64 {
        self.row + k * self.dx
    }

    /// Moves to the next row.
    #[inline]
    fn advance_row(&mut self) {
        self.row += self.dy;
    }
}

/// Narrows a row to the pixel interval where the three edge functions can all
/// be satisfied, as offsets from the left of the bounding box.
///
/// Each edge is linear in `x`, so `edge + k * step <= 0` solves directly for a
/// bound on `k`. Without this the loop steps its eight accumulators across the
/// empty corners of the bounding box, which for a typical triangle is half of
/// it.
///
/// The interval is widened by one pixel to absorb the rounding of the
/// divisions: coverage is still decided exactly by the per-pixel test, so this
/// only ever changes how many pixels are *visited*, never which are drawn.
fn covered_span(edge_row: &[f64; 3], step_x: &[f64; 3], span_len: usize) -> Option<(usize, usize)> {
    let last = (span_len - 1) as f64;
    let mut low = 0.0_f64;
    let mut high = last;

    for i in 0..3 {
        if step_x[i] > 0.0 {
            high = high.min(-edge_row[i] / step_x[i]);
        } else if step_x[i] < 0.0 {
            low = low.max(-edge_row[i] / step_x[i]);
        } else if edge_row[i] > 0.0 {
            return None; // this edge excludes the entire row
        }
    }

    let low = (low.floor() - 1.0).clamp(0.0, last);
    let high = (high.ceil() + 1.0).clamp(0.0, last);
    (low <= high).then_some((low as usize, high as usize))
}

/// Fills a triangle, sampling `texture` and modulating by `tint`, with a depth
/// test against the target's depth buffer.
///
/// Returns the number of pixels actually shaded — fragments that failed the
/// depth test do not count, which makes the figure a direct measure of
/// overdraw.
pub fn fill_triangle(
    target: &mut RenderTarget<'_>,
    vertices: [ScreenVertex; 3],
    texture: &Texture,
    tint: u32,
) -> u64 {
    let mut v = vertices;
    let mut area = signed_area(&v);

    if area == 0.0 || !area.is_finite() {
        return 0; // degenerate: zero area, or NaN leaking in from bad input
    }
    if area > 0.0 {
        // Normalize the winding so the coverage test below is a single
        // comparison per edge instead of a sign-dependent one.
        v.swap(1, 2);
        area = -area;
    }
    let inv_area = 1.0 / area;

    let Some((min_x, min_y, max_x, max_y)) = bounding_box(&v, target.width(), target.height())
    else {
        return 0; // entirely outside the viewport
    };

    // Edge functions sampled at the center of the top-left pixel of the box...
    let (px0, py0) = (min_x as f64 + 0.5, min_y as f64 + 0.5);
    let mut edge_row = [
        edge(&v[1], &v[2], px0, py0),
        edge(&v[2], &v[0], px0, py0),
        edge(&v[0], &v[1], px0, py0),
    ];
    // ...and their constant derivatives, so stepping replaces re-evaluating.
    let step_x = [-(v[2].y - v[1].y), -(v[0].y - v[2].y), -(v[1].y - v[0].y)];
    let step_y = [v[2].x - v[1].x, v[0].x - v[2].x, v[1].x - v[0].x];

    // Everything the shading needs, turned into a start value and two
    // constant gradients. Set up once per triangle; advanced by one add per
    // pixel from here on.
    let lerp = |values: [f64; 3]| Lerp::new(values, &edge_row, &step_x, &step_y, inv_area);
    let mut depth_of = lerp([v[0].z, v[1].z, v[2].z]);
    let mut inv_w = lerp([v[0].inv_w, v[1].inv_w, v[2].inv_w]);
    let mut u_over_w = lerp([v[0].u_over_w, v[1].u_over_w, v[2].u_over_w]);
    let mut v_over_w = lerp([v[0].v_over_w, v[1].v_over_w, v[2].v_over_w]);
    let mut light = lerp([
        v[0].intensity_over_w,
        v[1].intensity_over_w,
        v[2].intensity_over_w,
    ]);

    // Hoisted out of the pixel loop: the tint is constant across the triangle,
    // and white — by far the common case — skips the modulation entirely.
    let tinted = tint != WHITE;
    let tint_fixed = tint_to_fixed(tint);
    let span_len = max_x - min_x + 1;
    let mut shaded = 0;

    for y in min_y..=max_y {
        if let Some((first, last)) = covered_span(&edge_row, &step_x, span_len) {
            let k = first as f64;
            let mut e = [
                edge_row[0] + k * step_x[0],
                edge_row[1] + k * step_x[1],
                edge_row[2] + k * step_x[2],
            ];
            let mut z = depth_of.at(k);
            let mut w_inv = inv_w.at(k);
            let mut u = u_over_w.at(k);
            let mut tex_v = v_over_w.at(k);
            let mut intensity = light.at(k);

            let (colors, depths) = target.span_mut(y, min_x + first, last - first + 1);

            for (color, depth) in colors.iter_mut().zip(depths.iter_mut()) {
                // Area is negative after normalization, so "inside" is "every
                // edge function has the same (negative) sign".
                if e[0] <= 0.0 && e[1] <= 0.0 && e[2] <= 0.0 {
                    let z32 = z as f32;
                    if z32 < *depth {
                        *depth = z32;

                        // Undo the perspective divide: the attributes were
                        // interpolated as `attribute / w`, so multiplying by
                        // `w` recovers the value at this pixel.
                        let w = 1.0 / w_inv;
                        let texel = texture.sample(u * w, tex_v * w);
                        let lit = intensity_to_fixed(intensity * w);
                        *color = if tinted {
                            scale_packed(modulate_fixed(texel, tint_fixed), lit)
                        } else {
                            scale_packed(texel, lit)
                        };
                        shaded += 1;
                    }
                }

                e[0] += step_x[0];
                e[1] += step_x[1];
                e[2] += step_x[2];
                z += depth_of.dx;
                w_inv += inv_w.dx;
                u += u_over_w.dx;
                tex_v += v_over_w.dx;
                intensity += light.dx;
            }
        }

        edge_row[0] += step_y[0];
        edge_row[1] += step_y[1];
        edge_row[2] += step_y[2];
        depth_of.advance_row();
        inv_w.advance_row();
        u_over_w.advance_row();
        v_over_w.advance_row();
        light.advance_row();
    }

    shaded
}

/// Screen-space bounding box of the triangle, clamped to the viewport.
///
/// Returns `None` when nothing is left after clamping, which is the cheap
/// rejection path for off-screen geometry — and the reason the left/right/
/// top/bottom frustum planes never need real clipping.
fn bounding_box(
    v: &[ScreenVertex; 3],
    width: usize,
    height: usize,
) -> Option<(usize, usize, usize, usize)> {
    let min_x = v.iter().map(|p| p.x).fold(f64::INFINITY, f64::min).floor();
    let max_x = v
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil();
    let min_y = v.iter().map(|p| p.y).fold(f64::INFINITY, f64::min).floor();
    let max_y = v
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max)
        .ceil();

    // Clamped in f64 and compared before the cast: casting a negative float to
    // usize saturates to 0, which would silently turn an off-screen triangle
    // into a full-width one.
    let min_x = min_x.max(0.0);
    let min_y = min_y.max(0.0);
    let max_x = max_x.min(width as f64 - 1.0);
    let max_y = max_y.min(height as f64 - 1.0);

    (min_x <= max_x && min_y <= max_y).then_some((
        min_x as usize,
        min_y as usize,
        max_x as usize,
        max_y as usize,
    ))
}

/// Draws the three edges of a triangle as an overlay, ignoring the depth
/// buffer.
///
/// Debug aid: it makes tessellation, back-face culling and the extra triangles
/// produced by clipping directly visible.
pub fn draw_wireframe(target: &mut RenderTarget<'_>, v: &[ScreenVertex; 3], color: u32) {
    for i in 0..3 {
        let a = &v[i];
        let b = &v[(i + 1) % 3];
        draw_line(target, a.x, a.y, b.x, b.y, color);
    }
}

/// Bresenham line, in screen-space floating point coordinates.
///
/// Pixels outside the viewport are dropped by
/// [`RenderTarget::put_pixel`], so the endpoints do not need clipping.
pub fn draw_line(target: &mut RenderTarget<'_>, x0: f64, y0: f64, x1: f64, y1: f64, color: u32) {
    if ![x0, y0, x1, y1].iter().all(|c| c.is_finite()) {
        return;
    }

    let (mut x0, mut y0) = (x0.round() as i64, y0.round() as i64);
    let (x1, y1) = (x1.round() as i64, y1.round() as i64);

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let step_x = if x0 < x1 { 1 } else { -1 };
    let step_y = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;

    loop {
        target.put_pixel(x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        // Doubling the error is the integer form of "which axis is further
        // from the ideal line right now".
        let double_error = 2 * error;
        if double_error >= dy {
            error += dy;
            x0 += step_x;
        }
        if double_error <= dx {
            error += dx;
            y0 += step_y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vértice de tela com atributos neutros (`w = 1`, luz cheia).
    fn vertex(x: f64, y: f64, z: f64) -> ScreenVertex {
        ScreenVertex {
            x,
            y,
            z,
            inv_w: 1.0,
            u_over_w: 0.0,
            v_over_w: 0.0,
            intensity_over_w: 1.0,
        }
    }

    /// Alvo quadrado limpo, com os buffers vivos para inspeção.
    fn target_buffers(size: usize) -> (Vec<u32>, Vec<f32>) {
        (vec![0_u32; size * size], vec![f32::INFINITY; size * size])
    }

    /// Um triângulo que cobre metade da tela pinta pixels e respeita a área.
    #[test]
    fn fills_covered_pixels() {
        let (mut color, mut depth) = target_buffers(8);
        let mut target = RenderTarget::new(&mut color, &mut depth, 8, 8);
        let texture = Texture::white();

        let shaded = fill_triangle(
            &mut target,
            [
                vertex(0.0, 0.0, 0.0),
                vertex(8.0, 0.0, 0.0),
                vertex(0.0, 8.0, 0.0),
            ],
            &texture,
            0x00FFFFFF,
        );

        assert!(shaded > 0);
        assert_eq!(target.pixel(0, 0), Some(0x00FFFFFF), "canto coberto");
        assert_eq!(target.pixel(7, 7), Some(0x00000000), "canto oposto vazio");
    }

    /// A ordem dos vértices não muda o resultado — o winding é normalizado.
    #[test]
    fn winding_does_not_change_coverage() {
        let texture = Texture::white();
        let triangle = [
            vertex(1.0, 1.0, 0.0),
            vertex(7.0, 1.0, 0.0),
            vertex(1.0, 7.0, 0.0),
        ];

        let (mut c1, mut d1) = target_buffers(8);
        let ccw = fill_triangle(
            &mut RenderTarget::new(&mut c1, &mut d1, 8, 8),
            triangle,
            &texture,
            0x00FFFFFF,
        );

        let (mut c2, mut d2) = target_buffers(8);
        let cw = fill_triangle(
            &mut RenderTarget::new(&mut c2, &mut d2, 8, 8),
            [triangle[0], triangle[2], triangle[1]],
            &texture,
            0x00FFFFFF,
        );

        assert_eq!(ccw, cw);
        assert_eq!(c1, c2);
    }

    /// O z-buffer deixa passar o mais próximo e barra o mais distante.
    #[test]
    fn depth_test_keeps_the_nearest_fragment() {
        let (mut color, mut depth) = target_buffers(4);
        let mut target = RenderTarget::new(&mut color, &mut depth, 4, 4);
        let texture = Texture::white();
        let full = |z: f64| {
            [
                vertex(0.0, 0.0, z),
                vertex(8.0, 0.0, z),
                vertex(0.0, 8.0, z),
            ]
        };

        assert!(fill_triangle(&mut target, full(0.5), &texture, 0x00FFFFFF) > 0);
        assert_eq!(
            fill_triangle(&mut target, full(0.9), &texture, 0x00FFFFFF),
            0,
            "o mais distante não pode sobrescrever"
        );
        assert!(
            fill_triangle(&mut target, full(0.1), &texture, 0x00FFFFFF) > 0,
            "o mais próximo tem que passar"
        );
    }

    /// Triângulo degenerado (área zero) não pinta nada.
    #[test]
    fn degenerate_triangle_draws_nothing() {
        let (mut color, mut depth) = target_buffers(4);
        let mut target = RenderTarget::new(&mut color, &mut depth, 4, 4);

        let collinear = [
            vertex(0.0, 0.0, 0.0),
            vertex(2.0, 2.0, 0.0),
            vertex(4.0, 4.0, 0.0),
        ];
        assert_eq!(
            fill_triangle(&mut target, collinear, &Texture::white(), 0x00FFFFFF),
            0
        );
    }

    /// Geometria fora da tela é rejeitada pela bounding box, não desenhada.
    #[test]
    fn offscreen_triangle_is_rejected() {
        let (mut color, mut depth) = target_buffers(4);
        let mut target = RenderTarget::new(&mut color, &mut depth, 4, 4);

        let far_left = [
            vertex(-100.0, 0.0, 0.0),
            vertex(-90.0, 0.0, 0.0),
            vertex(-100.0, 10.0, 0.0),
        ];
        assert_eq!(
            fill_triangle(&mut target, far_left, &Texture::white(), 0x00FFFFFF),
            0
        );
        assert!(color.iter().all(|&c| c == 0));
    }

    /// A intensidade escurece a cor sem alterar o balanço dos canais.
    #[test]
    fn intensity_scales_the_color() {
        assert_eq!(scale_color(0x00FFFFFF, 0.5), 0x007F7F7F);
        assert_eq!(scale_color(0x00FFFFFF, 0.0), 0x00000000);
        assert_eq!(scale_color(0x00204060, 1.0), 0x00204060);
    }

    /// Modular por branco é neutro; por preto zera.
    #[test]
    fn modulation_by_white_is_neutral() {
        assert_eq!(modulate(0x00123456, 0x00FFFFFF), 0x00123456);
        assert_eq!(modulate(0x00123456, 0x00000000), 0x00000000);
    }

    /// A linha do wireframe atinge os dois extremos.
    #[test]
    fn line_reaches_both_endpoints() {
        let (mut color, mut depth) = target_buffers(8);
        let mut target = RenderTarget::new(&mut color, &mut depth, 8, 8);

        draw_line(&mut target, 0.0, 0.0, 7.0, 7.0, 0x00FF0000);

        assert_eq!(target.pixel(0, 0), Some(0x00FF0000));
        assert_eq!(target.pixel(7, 7), Some(0x00FF0000));
        assert_eq!(target.pixel(0, 7), Some(0x00000000));
    }
}
