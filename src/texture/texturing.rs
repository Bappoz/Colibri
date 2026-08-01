/// Textura decodificada em memória, pronta pra sampling. Guarda os pixels já
/// no formato `0x00RRGGBB` (mesmo do framebuffer) pra não converter por pixel
/// dentro do hot loop do rasterizador.
pub struct Texture {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl Texture {
    pub fn load(path: &str) -> Self {
        let img = image::open(path)
            .unwrap_or_else(|e| panic!("failed to load texture {path}: {e}"))
            .to_rgba8();
        let (width, height) = img.dimensions();
        let pixels = img
            .pixels()
            .map(|p| {
                let [r, g, b, _a] = p.0;
                (r as u32) << 16 | (g as u32) << 8 | b as u32
            })
            .collect();

        Self {
            width,
            height,
            pixels,
        }
    }

    /// Xadrez gerado em código, sem depender de nenhum arquivo -- ótimo pra
    /// testar correção de perspectiva: as linhas retas do padrão escancaram
    /// qualquer "escorregão" de UV sem correção.
    pub fn checkerboard(size: u32, cell: u32) -> Self {
        let mut pixels = Vec::with_capacity((size * size) as usize);
        for y in 0..size {
            for x in 0..size {
                let on = (x / cell + y / cell).is_multiple_of(2);
                pixels.push(if on { 0x00FFFFFF } else { 0x00FF0000 });
            }
        }
        Self {
            width: size,
            height: size,
            pixels,
        }
    }

    /// Nearest-neighbor. `u,v` fora de `[0,1]` repetem a textura (wrap).
    pub fn sample(&self, u: f64, v: f64) -> u32 {
        let u = u.rem_euclid(1.0);
        let v = v.rem_euclid(1.0);
        let x = ((u * self.width as f64) as u32).min(self.width - 1);
        let y = ((v * self.height as f64) as u32).min(self.height - 1);
        self.pixels[(y * self.width + x) as usize]
    }
}
