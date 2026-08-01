use crate::math::utils::Vec3d;

pub struct DirectionalLight {
    pub direction: Vec3d,
    pub ambient: f64, // 0.0 - 1.0
}

impl DirectionalLight {
    pub fn intensity_for(&self, normal: Vec3d) -> f64 {
        normal
            .dot_product(&-self.direction)
            .max(0.0)
            .max(self.ambient)
    }
}
