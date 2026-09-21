//! Entity-Component-System core.
//!
//! Three pieces, in the order they were built:
//!
//! | Piece | Answers |
//! |---|---|
//! | [`Entity`] | *Who* — a generational id that owns no data |
//! | [`SparseSet`] | *Where* — one packed column per component type |
//! | [`World`] | *Everything* — the columns, keyed by `TypeId` |
//! | [`Schedule`] | *When* — the systems, run in order every frame |
//!
//! A system is a function `(&mut World, dt)`. Reading one component while
//! writing another goes through [`World::query2`] / [`World::query2_mut`];
//! wider queries (three or more columns) are still to come.

pub mod entity;
pub mod schedule;
pub mod sparse_set;
pub mod world;

pub use entity::{Entity, EntityAllocator};
pub use schedule::Schedule;
pub use sparse_set::SparseSet;
pub use world::World;
