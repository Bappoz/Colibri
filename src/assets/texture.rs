//! CPU-side textures and sampling.

use crate::error::{Error, Result};

/// A decoded texture, ready for sampling.
///
/// Pixels are stored in the framebuffer's own `0x00RRGGBB` layout so the
/// rasterizer's inner loop never has to unpack a color: the alpha channel is
/// dropped at load time, once, instead of per sampled texel.
pub struct Texture {
    /// Width in texels; always >= 1.
    width: u32,
    /// Height in texels; always >= 1.
    height: u32,
    /// Row-major texels, `width * height` entries, `0x00RRGGBB` each.
    pixels: Vec<u32>,
}

impl Texture {
    /// Decodes an image file from disk.
    ///
    /// Any format supported by the `image` crate works; the alpha channel is
    /// discarded because the rasterizer is opaque-only for now.
    pub fn load(path: &str) -> Result<Self> {
        let img = image::open(path)
            .map_err(|e| Error::TextureLoad {
                path: path.to_string(),
                reason: e.to_string(),
            })?
            .to_rgba8();

        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            return Err(Error::TextureLoad {
                path: path.to_string(),
                reason: "image has zero width or height".to_string(),
            });
        }

        let pixels = img
            .pixels()
            .map(|p| {
                let [r, g, b, _a] = p.0;
                (r as u32) << 16 | (g as u32) << 8 | b as u32
            })
            .collect();

        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Generates a checkerboard without touching the filesystem.
    ///
    /// Straight lines are the harshest test for perspective-correct
    /// interpolation: without the `1/w` correction the pattern visibly bends
    /// along the diagonal of every quad.
    ///
    /// * `size` — side of the square texture, in texels.
    /// * `cell` — side of one checker cell, in texels.
    pub fn checkerboard(size: u32, cell: u32) -> Self {
        let size = size.max(1);
        let cell = cell.max(1);
        let mut pixels = Vec::with_capacity((size * size) as usize);
        for y in 0..size {
            for x in 0..size {
                let light = (x / cell + y / cell).is_multiple_of(2);
                pixels.push(if light { 0x00FFFFFF } else { 0x00FF0000 });
            }
        }
        Self {
            width: size,
            height: size,
            pixels,
        }
    }

    /// A single-texel white texture, used as the default when a mesh has no
    /// material — sampling it is a no-op tint.
    pub fn white() -> Self {
        Self {
            width: 1,
            height: 1,
            pixels: vec![0x00FFFFFF],
        }
    }

    /// Texture width in texels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Texture height in texels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Nearest-neighbour sample. Coordinates outside `[0, 1]` wrap around
    /// (`GL_REPEAT`), so tiling a mesh only needs UVs greater than one.
    ///
    /// This is the hottest function in the engine — it runs once per shaded
    /// pixel — hence the branch-free `rem_euclid` wrap and the pre-packed
    /// pixel format.
    #[inline]
    pub fn sample(&self, u: f64, v: f64) -> u32 {
        let u = u.rem_euclid(1.0);
        let v = v.rem_euclid(1.0);
        let x = ((u * self.width as f64) as u32).min(self.width - 1);
        let y = ((v * self.height as f64) as u32).min(self.height - 1);
        self.pixels[(y * self.width + x) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UVs fora de `[0,1]` repetem a textura em vez de estourar o índice.
    #[test]
    fn sampling_wraps_out_of_range_uvs() {
        let tex = Texture::checkerboard(4, 1);
        assert_eq!(tex.sample(0.1, 0.1), tex.sample(1.1, 2.1));
        assert_eq!(tex.sample(0.1, 0.1), tex.sample(-0.9, -1.9));
    }

    /// A borda superior de `u` não pode ler fora da linha.
    #[test]
    fn sampling_clamps_the_upper_edge() {
        let tex = Texture::checkerboard(2, 1);
        // 0.999... arredonda para o último texel, não para width.
        let _ = tex.sample(0.999_999_999, 0.999_999_999);
    }

    /// O xadrez alterna a cada célula, que é o que revela erro de perspectiva.
    #[test]
    fn checkerboard_alternates_per_cell() {
        let tex = Texture::checkerboard(4, 2);
        assert_ne!(tex.sample(0.1, 0.1), tex.sample(0.6, 0.1));
        assert_eq!(tex.sample(0.1, 0.1), tex.sample(0.6, 0.6));
    }

    /// A textura padrão é neutra: modular por ela não muda a cor.
    #[test]
    fn white_is_a_single_neutral_texel() {
        let tex = Texture::white();
        assert_eq!((tex.width(), tex.height()), (1, 1));
        assert_eq!(tex.sample(0.5, 0.5), 0x00FFFFFF);
    }
}
