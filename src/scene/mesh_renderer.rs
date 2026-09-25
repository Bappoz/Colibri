//! Geometry and how to shade it — the drawable half of an entity.

use crate::assets::{MeshHandle, TextureHandle};

/// Tint that leaves the sampled texture untouched (white, fully modulated).
pub const NO_TINT: u32 = 0x00FF_FFFF;

/// Geometry and how to shade it.
///
/// Attach it next to a [`Transform`](crate::scene::Transform) to make an
/// entity drawable; a `MeshRenderer` with no transform is skipped, since
/// there is nowhere to put it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshRenderer {
    /// Geometry to draw.
    pub mesh: MeshHandle,
    /// Texture sampled across that geometry.
    pub texture: TextureHandle,
    /// Color multiplied into the sampled texel, `0x00RRGGBB`. [`NO_TINT`]
    /// leaves the texture as it is.
    pub tint: u32,
}

impl MeshRenderer {
    /// An untinted renderer for `mesh`, sampling `texture`.
    pub const fn new(mesh: MeshHandle, texture: TextureHandle) -> Self {
        Self {
            mesh,
            texture,
            tint: NO_TINT,
        }
    }

    /// Builder-style tint.
    pub const fn with_tint(mut self, tint: u32) -> Self {
        self.tint = tint;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{Assets, Mesh, Texture};
    use crate::scene::{Scene, Transform};

    /// The tint stays per entity: two renderers sharing a mesh can still
    /// look different.
    #[test]
    fn the_tint_travels_with_the_renderer() {
        let mut assets = Assets::new();
        let mesh = assets.add_mesh(Mesh::textured_quad());
        let texture = assets.add_texture(Texture::white());

        let mut scene = Scene::new();
        let plain = scene.spawn_object(Transform::IDENTITY, MeshRenderer::new(mesh, texture));
        let tinted = scene.spawn_object(
            Transform::IDENTITY,
            MeshRenderer::new(mesh, texture).with_tint(0x00FF_0000),
        );

        assert_eq!(
            scene.world.get::<MeshRenderer>(plain).unwrap().tint,
            NO_TINT
        );
        assert_eq!(
            scene.world.get::<MeshRenderer>(tinted).unwrap().tint,
            0x00FF_0000
        );
    }
}
