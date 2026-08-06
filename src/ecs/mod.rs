//! Entity-Component-System core.
//!
//! This is the seed of Phase C of the roadmap. So far it holds only the
//! **entity**: a generational id with no data attached. Component storages and
//! systems land in the next stages; until then [`crate::scene::Scene`] plays
//! the role of a hand-written, single-archetype storage so the renderer can
//! already be driven by entities instead of by a hard-coded mesh.

pub mod entity;

pub use entity::{Entity, EntityAllocator};
