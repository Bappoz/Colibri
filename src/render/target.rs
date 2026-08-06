//! The surface a frame is drawn into: one color buffer and one depth buffer.

/// A mutable view over the two buffers that make up a frame.
///
/// The color buffer is owned by the windowing layer (`softbuffer` hands out a
/// mapped slice each frame) and the depth buffer is owned by the
/// [`crate::render::Renderer`], so the target borrows both rather than owning
/// either. It exists to keep the "same length, same dimensions" invariant in
/// one place instead of threading four arguments through the rasterizer.
pub struct RenderTarget<'a> {
    /// Row-major pixels, `0x00RRGGBB`, `width * height` entries.
    color: &'a mut [u32],
    /// Row-major depth, same layout as `color`. Holds NDC `z` in `[-1, 1]`,
    /// with `f32::INFINITY` meaning "nothing drawn here yet".
    depth: &'a mut [f32],
    /// Width in pixels.
    width: usize,
    /// Height in pixels.
    height: usize,
}

impl<'a> RenderTarget<'a> {
    /// Binds a color and a depth buffer as one target.
    ///
    /// # Panics
    ///
    /// If either slice is shorter than `width * height`. This is a programming
    /// error in the caller (a missed resize), not a runtime condition, so it
    /// fails loudly rather than silently drawing into the wrong pixels.
    pub fn new(color: &'a mut [u32], depth: &'a mut [f32], width: usize, height: usize) -> Self {
        let pixels = width * height;
        assert!(
            color.len() >= pixels && depth.len() >= pixels,
            "render target buffers are smaller than {width}x{height}"
        );
        Self {
            color,
            depth,
            width,
            height,
        }
    }

    /// Width in pixels.
    #[inline]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Height in pixels.
    #[inline]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Resets both buffers for a new frame: the given background color, and a
    /// depth of `+inf` so the first fragment at any pixel always wins.
    pub fn clear(&mut self, color: u32) {
        self.color[..self.width * self.height].fill(color);
        self.depth[..self.width * self.height].fill(f32::INFINITY);
    }

    /// Borrows a horizontal run of pixels from row `y`, from `x` inclusive for
    /// `len` pixels, as a color slice and a depth slice.
    ///
    /// This is the rasterizer's hot path: iterating two slices lets the
    /// compiler hoist the bounds checks out of the inner loop, which indexing
    /// `buffer[y * width + x]` per pixel does not.
    ///
    /// # Panics
    ///
    /// If the span leaves the buffer — the caller is expected to have clamped
    /// it to the viewport already.
    #[inline]
    pub fn span_mut(&mut self, y: usize, x: usize, len: usize) -> (&mut [u32], &mut [f32]) {
        let start = y * self.width + x;
        let end = start + len;
        (&mut self.color[start..end], &mut self.depth[start..end])
    }

    /// Writes one pixel, ignoring the depth buffer.
    ///
    /// Out-of-bounds coordinates are dropped instead of panicking, because the
    /// overlays that use this (wireframe) draw from unclamped screen-space
    /// coordinates.
    #[inline]
    pub fn put_pixel(&mut self, x: i64, y: i64, color: u32) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        self.color[y as usize * self.width + x as usize] = color;
    }

    /// Reads one pixel back. Returns `None` outside the viewport. Intended for
    /// tests and debugging, not for the frame loop.
    pub fn pixel(&self, x: i64, y: i64) -> Option<u32> {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return None;
        }
        Some(self.color[y as usize * self.width + x as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `clear` zera a cor e devolve a profundidade ao infinito.
    #[test]
    fn clear_resets_both_buffers() {
        let mut color = vec![0xFFFFFF_u32; 4];
        let mut depth = vec![0.0_f32; 4];
        let mut target = RenderTarget::new(&mut color, &mut depth, 2, 2);

        target.clear(0x101010);

        assert!(color.iter().all(|&c| c == 0x101010));
        assert!(depth.iter().all(|&d| d.is_infinite()));
    }

    /// `span_mut` endereça a linha certa do buffer.
    #[test]
    fn span_addresses_the_right_row() {
        let mut color = vec![0_u32; 6];
        let mut depth = vec![0.0_f32; 6];
        let mut target = RenderTarget::new(&mut color, &mut depth, 3, 2);

        let (row, _) = target.span_mut(1, 1, 2);
        row.fill(0xABCDEF);

        assert_eq!(color, vec![0, 0, 0, 0, 0xABCDEF, 0xABCDEF]);
    }

    /// Pixel fora da tela é descartado em vez de estourar.
    #[test]
    fn out_of_bounds_writes_are_dropped() {
        let mut color = vec![0_u32; 4];
        let mut depth = vec![0.0_f32; 4];
        let mut target = RenderTarget::new(&mut color, &mut depth, 2, 2);

        target.put_pixel(-1, 0, 0xFF0000);
        target.put_pixel(0, 9, 0xFF0000);
        target.put_pixel(1, 1, 0x00FF00);

        assert_eq!(target.pixel(1, 1), Some(0x00FF00));
        assert_eq!(target.pixel(5, 5), None);
    }
}
