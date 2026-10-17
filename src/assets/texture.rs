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
    /// `width` pre-widened to `f64`. Sampling runs once per shaded pixel, and
    /// an integer-to-float conversion there is pure waste.
    width_f: f64,
    /// `height` pre-widened to `f64`; see `width_f`.
    height_f: f64,
    /// `width - 1` when the width is a power of two, otherwise `0`.
    ///
    /// Wrapping a texel coordinate is a modulo, and an integer division in the
    /// hot loop costs tens of cycles. Power-of-two sizes — every procedural
    /// texture and most authored ones — reduce it to a bitwise AND; the zero
    /// marks the slow, general path.
    wrap_mask_x: u32,
    /// `height - 1` when the height is a power of two, otherwise `0`.
    wrap_mask_y: u32,
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

        Ok(Self::from_pixels(width, height, pixels))
    }

    /// Converts decoded glTF image data to the renderer's opaque RGB format.
    pub(crate) fn from_gltf_image(path: &str, image: &gltf::image::Data) -> Result<Self> {
        use gltf::image::Format;

        let (channels, bytes_per_channel) = match image.format {
            Format::R8 => (1, 1),
            Format::R8G8 => (2, 1),
            Format::R8G8B8 => (3, 1),
            Format::R8G8B8A8 => (4, 1),
            Format::R16 => (1, 2),
            Format::R16G16 => (2, 2),
            Format::R16G16B16 => (3, 2),
            Format::R16G16B16A16 => (4, 2),
            Format::R32G32B32FLOAT => (3, 4),
            Format::R32G32B32A32FLOAT => (4, 4),
        };
        let pixel_count = (image.width as usize).checked_mul(image.height as usize);
        let bytes_per_pixel = channels * bytes_per_channel;
        let invalid_image = || Error::GltfLoad {
            path: path.to_string(),
            reason: format!(
                "decoded image has invalid dimensions or data length ({}x{}, {:?})",
                image.width, image.height, image.format
            ),
        };
        let pixel_count = pixel_count.ok_or_else(&invalid_image)?;
        let expected_bytes = pixel_count
            .checked_mul(bytes_per_pixel)
            .ok_or_else(&invalid_image)?;

        if image.width == 0 || image.height == 0 || expected_bytes != image.pixels.len() {
            return Err(invalid_image());
        }

        let mut pixels = Vec::with_capacity(pixel_count);
        for pixel in image.pixels.chunks_exact(bytes_per_pixel) {
            let mut color = [0u8; 4];
            for channel in 0..channels {
                let start = channel * bytes_per_channel;
                color[channel] =
                    gltf_channel_to_u8(&pixel[start..start + bytes_per_channel], image.format);
            }

            // Single-channel and luminance-alpha formats are grayscale.
            let (r, g, b) = match channels {
                1 | 2 => (color[0], color[0], color[0]),
                _ => (color[0], color[1], color[2]),
            };
            pixels.push((r as u32) << 16 | (g as u32) << 8 | b as u32);
        }

        Ok(Self::from_pixels(image.width, image.height, pixels))
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
        Self::from_pixels(size, size, pixels)
    }

    /// A single-texel white texture, used as the default when a mesh has no
    /// material — sampling it is a no-op tint.
    pub fn white() -> Self {
        Self::from_pixels(1, 1, vec![0x00FFFFFF])
    }

    /// Assembles a texture from already-packed texels, deriving the cached
    /// float dimensions.
    ///
    /// # Panics
    ///
    /// If `pixels` does not hold exactly `width * height` texels — an
    /// inconsistency the sampler cannot detect later.
    fn from_pixels(width: u32, height: u32, pixels: Vec<u32>) -> Self {
        assert_eq!(
            pixels.len(),
            (width * height) as usize,
            "texture pixel count does not match {width}x{height}"
        );
        Self {
            width,
            height,
            width_f: width as f64,
            height_f: height as f64,
            wrap_mask_x: if width.is_power_of_two() {
                width - 1
            } else {
                0
            },
            wrap_mask_y: if height.is_power_of_two() {
                height - 1
            } else {
                0
            },
            pixels,
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
    /// pixel — hence the pre-packed pixel format and the cached float
    /// dimensions.
    ///
    /// The wrap happens in *texel* space rather than on the `[0, 1]`
    /// coordinate: scaling first and wrapping the integer afterwards replaces
    /// a floating-point remainder plus a clamp with one `floor` and one
    /// bitwise AND. `rem_euclid` on the `f64` cost roughly 3 ms per 1080p
    /// frame of full-screen geometry.
    #[inline]
    pub fn sample(&self, u: f64, v: f64) -> u32 {
        let x = wrap_texel(u * self.width_f, self.width, self.wrap_mask_x);
        let y = wrap_texel(v * self.height_f, self.height, self.wrap_mask_y);
        self.pixels[(y * self.width + x) as usize]
    }
}

/// Converts one decoded glTF color channel to the renderer's 8-bit channel.
fn gltf_channel_to_u8(bytes: &[u8], format: gltf::image::Format) -> u8 {
    use gltf::image::Format;

    match format {
        Format::R8 | Format::R8G8 | Format::R8G8B8 | Format::R8G8B8A8 => bytes[0],
        Format::R16 | Format::R16G16 | Format::R16G16B16 | Format::R16G16B16A16 => {
            let value = u16::from_le_bytes([bytes[0], bytes[1]]) as u32;
            ((value * 255 + 32_767) / 65_535) as u8
        }
        Format::R32G32B32FLOAT | Format::R32G32B32A32FLOAT => {
            let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            if value.is_nan() {
                0
            } else {
                (value.clamp(0.0, 1.0) * 255.0).round() as u8
            }
        }
    }
}

/// Wraps a scaled texel coordinate into `0..size`, `GL_REPEAT` style.
///
/// `floor` rather than truncation, so a negative coordinate wraps around the
/// far edge instead of folding back toward zero. `mask` is the power-of-two
/// fast path; `0` selects the general modulo.
#[inline]
fn wrap_texel(scaled: f64, size: u32, mask: u32) -> u32 {
    let index = scaled.floor() as i64;
    if mask != 0 {
        index as u32 & mask
    } else {
        index.rem_euclid(size as i64) as u32
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
        // E coordenadas absurdas não podem indexar fora do buffer.
        let _ = tex.sample(1e18, -1e18);
        let _ = tex.sample(f64::NAN, f64::INFINITY);
    }

    /// Textura não potência de dois cai no caminho geral e continua correta.
    #[test]
    fn non_power_of_two_still_wraps() {
        let tex = Texture::checkerboard(6, 2);
        assert_eq!(tex.sample(0.1, 0.1), tex.sample(1.1, 2.1));
        assert_eq!(tex.sample(0.1, 0.1), tex.sample(-0.9, -1.9));
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
