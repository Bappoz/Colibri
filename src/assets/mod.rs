//! Asset types and the registry that owns them.
//!
//! Nothing in the scene stores a [`Mesh`] or a [`Texture`] directly — it
//! stores a *handle*. Two entities pointing at the same cube share one copy of
//! the geometry, and the borrow checker stops fighting the renderer, which
//! needs to read the assets while mutating the scene.
//!
//! Handles are plain indices into [`Assets`]. That is enough while assets are
//! only ever added; reloading and eviction arrive with the asset stage of the
//! roadmap, and will need a generation just like [`crate::ecs::Entity`].

pub mod mesh;
pub mod texture;

pub use mesh::{Mesh, Vertex};
pub use texture::Texture;

use crate::error::Result;

/// A handle to a [`Mesh`] stored in [`Assets`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshHandle(u32);

/// A handle to a [`Texture`] stored in [`Assets`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(u32);

/// Owns every mesh and texture loaded by the application.
///
/// Handles stay valid for the lifetime of the registry: nothing is ever
/// removed, only appended.
#[derive(Default)]
pub struct Assets {
    /// Meshes, addressed by [`MeshHandle`].
    meshes: Vec<Mesh>,
    /// Textures, addressed by [`TextureHandle`].
    textures: Vec<Texture>,
}

impl Assets {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a mesh and returns its handle.
    pub fn add_mesh(&mut self, mesh: Mesh) -> MeshHandle {
        self.meshes.push(mesh);
        MeshHandle(self.meshes.len() as u32 - 1)
    }

    /// Stores a texture and returns its handle.
    pub fn add_texture(&mut self, texture: Texture) -> TextureHandle {
        self.textures.push(texture);
        TextureHandle(self.textures.len() as u32 - 1)
    }

    /// Loads a `.obj` from disk and stores it.
    pub fn load_mesh(&mut self, path: &str) -> Result<MeshHandle> {
        Ok(self.add_mesh(Mesh::load_from_obj(path)?))
    }

    /// Loads an image from disk and stores it.
    pub fn load_texture(&mut self, path: &str) -> Result<TextureHandle> {
        Ok(self.add_texture(Texture::load(path)?))
    }

    /// Borrows a mesh.
    ///
    /// # Panics
    ///
    /// Only if the handle came from a different [`Assets`] instance — handles
    /// minted here stay valid forever, since nothing is ever removed.
    #[inline]
    pub fn mesh(&self, handle: MeshHandle) -> &Mesh {
        &self.meshes[handle.0 as usize]
    }

    /// Borrows a texture. Panics under the same conditions as
    /// [`Assets::mesh`].
    #[inline]
    pub fn texture(&self, handle: TextureHandle) -> &Texture {
        &self.textures[handle.0 as usize]
    }

    /// Number of meshes currently stored.
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Number of textures currently stored.
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Total triangle count across every stored mesh — a cheap sanity figure
    /// for the startup log.
    pub fn total_triangles(&self) -> usize {
        self.meshes.iter().map(Mesh::triangle_count).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Handles diferentes endereçam assets diferentes, na ordem de inserção.
    #[test]
    fn handles_address_distinct_assets() {
        let mut assets = Assets::new();
        let quad = assets.add_mesh(Mesh::textured_quad());
        let empty = assets.add_mesh(Mesh::default());

        assert_ne!(quad, empty);
        assert_eq!(assets.mesh(quad).triangle_count(), 2);
        assert_eq!(assets.mesh(empty).triangle_count(), 0);
        assert_eq!(assets.mesh_count(), 2);
    }

    /// Duas entidades podem apontar para a mesma geometria sem copiá-la.
    #[test]
    fn the_same_handle_is_shareable() {
        let mut assets = Assets::new();
        let handle = assets.add_texture(Texture::white());

        assert_eq!(assets.texture(handle).sample(0.0, 0.0), 0x00FFFFFF);
        assert_eq!(assets.texture(handle).width(), 1);
        assert_eq!(assets.texture_count(), 1);
    }
}
