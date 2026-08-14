//! Entity-Component-System core.
//!
//! Three pieces, in the order they were built:
//!
//! | Piece | Answers |
//! |---|---|
//! | [`Entity`] | *Who* — a generational id that owns no data |
//! | [`SparseSet`] | *Where* — one packed column per component type |
//! | [`World`] | *Everything* — the columns, keyed by `TypeId` |
//!
//! Systems are still plain functions taking `&mut World` — see
//! [`crate::scene::spin_system`]. The scheduler, and the multi-component
//! queries that let a system borrow two columns at once, are the next stage.

pub mod entity;
pub mod sparse_set;
pub mod world;

pub use entity::{Entity, EntityAllocator};
pub use sparse_set::SparseSet;
pub use world::World;
