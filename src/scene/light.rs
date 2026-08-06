//! Lighting. One directional light for now — enough to read the shape of a
//! mesh, which is all the software rasterizer needs before PBR.

use crate::math::Vec3d;

/// A light infinitely far away, so every surface receives it from the same
/// direction — the sun model.
#[derive(Debug, Clone, Copy)]
pub struct DirectionalLight {
    /// Unit vector pointing *along* the travel of the light (from the source
    /// towards the scene). A surface is lit when its normal opposes it.
    pub direction: Vec3d,
    /// Floor brightness in `[0, 1]`, applied to surfaces facing away so they
    /// do not read as solid black.
    pub ambient: f64,
}

impl Default for DirectionalLight {
    /// A light coming down and slightly from the side, with enough ambient to
    /// keep back faces readable.
    fn default() -> Self {
        Self {
            direction: Vec3d::new(0.5, -1.0, 0.3).normalize(),
            ambient: 0.35,
        }
    }
}

impl DirectionalLight {
    /// Builds a light travelling along `direction`; the vector is normalized
    /// for you, since the diffuse term assumes unit length.
    pub fn new(direction: Vec3d, ambient: f64) -> Self {
        Self {
            direction: direction.normalize(),
            ambient: ambient.clamp(0.0, 1.0),
        }
    }

    /// Lambertian diffuse term for a surface with the given world-space
    /// normal, clamped to `[ambient, 1]`.
    ///
    /// `normal · (-direction)` is the cosine of the angle between the surface
    /// and the incoming light: 1 facing it head on, 0 edge on, negative when
    /// facing away — which is where the ambient floor takes over.
    #[inline]
    pub fn intensity_for(&self, normal: Vec3d) -> f64 {
        normal.dot(&-self.direction).max(self.ambient).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Superfície de frente para a luz recebe intensidade máxima.
    #[test]
    fn surface_facing_the_light_is_fully_lit() {
        let light = DirectionalLight::new(Vec3d::new(0.0, -1.0, 0.0), 0.0);
        assert!((light.intensity_for(Vec3d::UP) - 1.0).abs() < 1e-12);
    }

    /// Superfície de costas cai para o ambiente, nunca para valor negativo.
    #[test]
    fn surface_facing_away_falls_back_to_ambient() {
        let light = DirectionalLight::new(Vec3d::new(0.0, -1.0, 0.0), 0.25);
        assert!((light.intensity_for(-Vec3d::UP) - 0.25).abs() < 1e-12);
    }

    /// A intensidade nunca escapa de `[0, 1]` — o rasterizador conta com isso.
    #[test]
    fn intensity_stays_in_range() {
        let light = DirectionalLight::new(Vec3d::new(0.0, -1.0, 0.0), 2.0);
        assert!((0.0..=1.0).contains(&light.intensity_for(Vec3d::UP)));
        assert!((0.0..=1.0).contains(&light.intensity_for(-Vec3d::UP)));
    }
}
