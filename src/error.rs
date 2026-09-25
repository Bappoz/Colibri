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
    /// A glTF node carries a rotation or a non-uniform scale — the engine's
    /// `Transform` only understands translation and uniform scale until the
    /// animation stage brings in quaternions.
    UnsupportedNodeTransform {
        /// Path the engine tried to open.
        path: String,
        /// The node's name, when the file bothered to name it.
        node: String,
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
            Error::UnsupportedNodeTransform { path, node } => {
                write!(
                    f,
                    "node '{node}' in '{path}' has a rotation or non-uniform scale, \
                 which Transform (Euler-angle) cannot represent yet"
                )
            }
        }
    }
}

impl std::error::Error for Error {}
