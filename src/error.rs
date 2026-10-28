//! Error type shared by every fallible entry point of the engine.
//!
//! Colibri hand-rolls its error enum instead of pulling in `thiserror`: the
//! surface is small and the crate has a strict "no dependency without a
//! reason" rule. Asset loading is the one place where failure is expected
//! (a missing or malformed file), so it must never panic deep inside the
//! frame loop — it returns a [`Result`] the caller can report cleanly.

use std::fmt;

/// Convenience alias for results produced by the engine.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong while bringing the engine up.
///
/// Rendering itself is infallible by design: once the assets are loaded and
/// the surface exists, a frame cannot fail.
#[derive(Debug)]
pub enum Error {
    /// An `.obj` file could not be read or parsed.
    MeshLoad {
        /// Path the engine tried to open.
        path: String,
        /// Message from the underlying loader.
        reason: String,
    },
    /// An `.obj` file parsed, but contained no drawable geometry.
    EmptyMesh {
        /// Path the engine tried to open.
        path: String,
    },
    /// An image file could not be read or decoded.
    TextureLoad {
        /// Path the engine tried to open.
        path: String,
        /// Message from the underlying decoder.
        reason: String,
    },
    /// The platform failed to hand back a drawable surface for the window.
    Surface(String),
    /// A `.gltf`/`.glb` file could not be read or parsed.
    GltfLoad {
        /// Path that the engine tried to open.
        path: String,
        /// Message from the underlying loader.
        reason: String,
    },
    /// A glTF file parsed, but its default scene has no nodes to spawn.
    EmptyGltfScene {
        /// Path of the scene file.
        path: String,
    },
    /// A model path has no supported extension.
    UnsupportedModelFormat {
        /// Path the engine tried to open.
        path: String,
    },
    /// A glTF node contains a zero-length or non-finite rotation quaternion.
    InvalidNodeRotation {
        /// Path the engine tried to open.
        path: String,
        /// The node's name, when the file bothered to name it.
        node: String,
    },
    /// A glTF base-color texture references a UV set other than the one the
    /// mesh format currently stores.
    UnsupportedGltfTexCoord {
        /// Path the engine tried to open.
        path: String,
        /// The node's name, when the file bothered to name it.
        node: String,
        /// UV set referenced by the material.
        tex_coord: u32,
    },
    /// A glTF mesh primitive could not be converted into a triangle mesh.
    GltfPrimitive {
        /// Path the engine tried to open.
        path: String,
        /// The node's name, when the file bothered to name it.
        node: String,
        /// Index of the mesh containing the primitive.
        mesh_index: usize,
        /// Index of the failing primitive in its mesh.
        primitive_index: usize,
        /// Why the primitive is not supported or valid.
        reason: String,
    },
}

impl fmt::Display for Error {
    /// Renders a message aimed at the terminal, always naming the offending
    /// path so the user knows which asset to fix.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::MeshLoad { path, reason } => {
                write!(f, "failed to load mesh '{path}': {reason}")
            }
            Error::EmptyMesh { path } => {
                write!(f, "mesh '{path}' contains no triangles")
            }
            Error::TextureLoad { path, reason } => {
                write!(f, "failed to load texture '{path}': {reason}")
            }
            Error::Surface(reason) => write!(f, "failed to set up the render surface: {reason}"),
            Error::GltfLoad { path, reason } => {
                write!(f, "failed to load glTF scene '{path}': {reason}")
            }
            Error::EmptyGltfScene { path } => {
                write!(f, "glTF scene '{path}' has no nodes in its default scene")
            }
            Error::UnsupportedModelFormat { path } => write!(
                f,
                "unsupported model format for '{path}'; expected .obj, .gltf, or .glb"
            ),
            Error::InvalidNodeRotation { path, node } => write!(
                f,
                "node '{node}' in '{path}' has an invalid rotation quaternion"
            ),
            Error::UnsupportedGltfTexCoord {
                path,
                node,
                tex_coord,
            } => write!(
                f,
                "node '{node}' in '{path}' uses glTF texture coordinates set {tex_coord}; \
                 only set 0 is supported"
            ),
            Error::GltfPrimitive {
                path,
                node,
                mesh_index,
                primitive_index,
                reason,
            } => write!(
                f,
                "cannot import primitive {primitive_index} of mesh {mesh_index} on node \
                 '{node}' from '{path}': {reason}"
            ),
        }
    }
}

impl std::error::Error for Error {}
