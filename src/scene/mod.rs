//! What there is to draw: entities carrying a transform and a mesh, plus the
//! camera and light that observe them.
//!
//! A [`Scene`] is a [`World`](crate::ecs::World) — the entities and their
//! component columns — next to the handful of things that are *not* per
//! entity: the light. The camera lives *inside* the world too, as the sole
//! entity carrying a [`Camera`] component, fetched through
//! [`Scene::camera`]/[`Scene::camera_mut`].
//!
//! # Components
//!
//! | Component | Meaning | Lives in |
//! |---|---|---|
//! | [`Transform`] | Where the entity is in world space | [`transform`] |
//! | [`MeshRenderer`] | What geometry to draw there, and how to shade it | [`mesh_renderer`] |
//! | [`Spin`] | Radians per second added to the rotation | [`spin`] |
//! | [`Camera`] | Lens and orientation of the scene's single camera | [`camera`] |
//! | [`Parent`] | Which entity this one's transform is relative to | [`hierarchy`] |
//!
//! They are independent on purpose: the renderer draws whoever has both a
//! transform and a mesh, [`spin_system`] turns whoever has both a transform
//! and a spin, and an entity carrying only one of them is perfectly valid —
//! which is exactly what a single bundled `RenderObject` could not express.

pub mod camera;
pub mod core;
pub mod hierarchy;
pub mod light;
pub mod mesh_renderer;
pub mod spin;
pub mod transform;

pub use camera::Camera;
pub use core::Scene;
pub use hierarchy::{Parent, world_matrix};
pub use light::DirectionalLight;
pub use mesh_renderer::{MeshRenderer, NO_TINT};
pub use spin::{Spin, spin_system};
pub use transform::Transform;
