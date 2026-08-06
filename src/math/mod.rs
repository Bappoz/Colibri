//! Hand-rolled linear algebra: vectors and 4x4 matrices.
//!
//! Colibri deliberately ships its own math instead of depending on `glam` or
//! `nalgebra` — implementing the transform pipeline is the point of the
//! project. The space conventions shared by every type live in
//! [`matrix`]'s module documentation; read them before adding a new transform.

pub mod matrix;
pub mod vec;

pub use matrix::Mat4x4;
pub use vec::{Vec3d, Vec4d};
