//! The component world: entities plus one [`SparseSet`] column per component
//! type.
//!
//! ```text
//!   columns: TypeId::of::<Transform>()    → SparseSet<Transform>
//!            TypeId::of::<MeshRenderer>() → SparseSet<MeshRenderer>
//!            TypeId::of::<Spin>()         → SparseSet<Spin>
//! ```
//!
//! Columns of different `T` are different types, so they cannot share a map
//! directly. The way out is type erasure: store them as
//! `Box<dyn ComponentColumn>` keyed by [`TypeId`], and downcast back to the
//! concrete `SparseSet<T>` on access. The downcast checks the type id at
//! runtime, so reading a column as the wrong type is impossible, not merely
//! unlikely.
//!
//! A component type needs no trait, no derive and no registration: the first
//! `insert::<T>` creates its column.

use std::any::{Any, TypeId};
use std::collections::HashMap;

use crate::ecs::{Entity, EntityAllocator, SparseSet};

/// What the world can do to a column without knowing its component type.
///
/// Only [`World::despawn`] and the diagnostics need this; every typed access
/// downcasts to `SparseSet<T>` first and goes through its own API.
trait ComponentColumn: Any {
    /// Drops this entity's component, if it has one.
    fn remove_entity(&mut self, entity: Entity);
    /// How many entities are in this column.
    fn component_count(&self) -> usize;
    /// Bridge to the concrete `SparseSet<T>`.
    fn as_any(&self) -> &dyn Any;
    /// Mutable bridge to the concrete `SparseSet<T>`.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: 'static> ComponentColumn for SparseSet<T> {
    fn remove_entity(&mut self, entity: Entity) {
        self.remove(entity);
    }

    fn component_count(&self) -> usize {
        self.len()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Entities and their components.
#[derive(Default)]
pub struct World {
    /// Mints and recycles the ids; the authority on who is alive.
    entities: EntityAllocator,
    /// One [`SparseSet<T>`] per component type, keyed by `TypeId::of::<T>()`.
    columns: HashMap<TypeId, Box<dyn ComponentColumn>>,
}

impl World {
    /// An empty world.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mints a fresh entity with no components attached.
    pub fn spawn(&mut self) -> Entity {
        self.entities.spawn()
    }

    /// Kills an entity and drops every component it owned.
    ///
    /// Returns `false` for a stale handle, leaving the world untouched. The
    /// sweep is O(columns), not O(entities): each column removes in O(1).
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.entities.despawn(entity) {
            return false;
        }
        for column in self.columns.values_mut() {
            column.remove_entity(entity);
        }
        true
    }

    /// Whether the handle still names a live entity.
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.entities.is_alive(entity)
    }

    /// Iterates over the live entities, whatever components they carry.
    pub fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.entities.iter()
    }

    /// Number of live entities.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Whether no entity is alive.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Attaches `value` to `entity`, creating the column on first use.
    ///
    /// Returns `false` — changing nothing — when the handle is stale, so a
    /// component can never outlive the entity it describes.
    pub fn insert<T: 'static>(&mut self, entity: Entity, value: T) -> bool {
        if !self.entities.is_alive(entity) {
            return false;
        }
        self.column_mut::<T>().insert(entity, value);
        true
    }

    /// Detaches and returns the `T` of `entity`.
    pub fn remove<T: 'static>(&mut self, entity: Entity) -> Option<T> {
        self.column_opt_mut::<T>()?.remove(entity)
    }

    /// Borrows the `T` of `entity`.
    pub fn get<T: 'static>(&self, entity: Entity) -> Option<&T> {
        self.column_opt::<T>()?.get(entity)
    }

    /// Mutably borrows the `T` of `entity`.
    pub fn get_mut<T: 'static>(&mut self, entity: Entity) -> Option<&mut T> {
        self.column_opt_mut::<T>()?.get_mut(entity)
    }

    /// Whether `entity` carries a `T`.
    pub fn contains<T: 'static>(&self, entity: Entity) -> bool {
        self.column_opt::<T>().is_some_and(|c| c.contains(entity))
    }

    /// Iterates every `(entity, &T)` — the single-component query.
    ///
    /// The order is the column's dense order, which is not entity order and is
    /// not stable across removals. A system that needs a second component
    /// looks it up by entity inside the loop; multi-component queries that
    /// borrow two columns at once arrive with the next roadmap stage.
    pub fn iter<T: 'static>(&self) -> impl Iterator<Item = (Entity, &T)> + '_ {
        self.column_opt::<T>()
            .into_iter()
            .flat_map(|column| column.iter())
    }

    /// Mutable counterpart of [`World::iter`].
    pub fn iter_mut<T: 'static>(&mut self) -> impl Iterator<Item = (Entity, &mut T)> + '_ {
        self.column_opt_mut::<T>()
            .into_iter()
            .flat_map(|column| column.iter_mut())
    }

    /// Borrows the one entity expected to carry a `T`.
    ///
    /// For resources that live as a single component — the camera, for
    /// instance — instead of a special field outside the world. Panics when
    /// zero or more than one entity carries `T`: both are bugs, not states the
    /// caller should have to handle.
    pub fn single<T: 'static>(&self) -> &T {
        let mut it = self.iter::<T>();
        let (_, value) = it.next().expect("single::<T>() found no entity");
        assert!(it.next().is_none(), "single::<T>() found more than one entity");
        value
    }
    /// Mutable counterpart of [`World::single`]
    pub fn single_mut<T: 'static>(&mut self) -> &mut T {
        let mut it = self.iter_mut::<T>();
        let (_, value) = it.next().expect("single_mut::<T>() found no entity");
        assert!(it.next().is_none(), "single::<T>() found more than one entity");
        value
    }


    /// Iterates every entityy carrying both `A` and a `B`.
    /// The walk goes over the `A` column and looks each entity up in `B`, so the order
    /// of the type parameters is the order of the loop: put the rarer component first.
    pub fn query2<A: 'static, B: 'static>(&self) -> impl Iterator<Item = (Entity, &A, &B)> + '_ {
        // 'zip' collapses "either was never created" into 'None',
        // which 'into_iter().flat_map' turns into a empty iteration
        let columns = self.column_opt::<A>().zip(self.column_opt::<B>());
        columns.into_iter().flat_map(|(a, b)| {
            a.iter()
                .filter_map(move |(entity, a)| Some((entity, a, b.get(entity)?)))
        })
    }

    /// Like [`World::query2`], but yields `&mut B`.
    ///
    /// Reads `A`, writes `B`. The walk goes over the `B` column: an iterator
    /// cannot hand out `&mut` obtained through a per-entity `get_mut` (the
    /// borrow would have to outlive `next`), but `iter_mut` can, and the
    /// shared side is just a lookup.
    ///
    /// # Panics
    ///
    /// When `A` and `B` are the same type: that would be a shared and a mutable
    /// borrow of one column.
    pub fn query2_mut<A: 'static, B: 'static>(
        &mut self,
    ) -> impl Iterator<Item = (Entity, &A, &mut B)> + '_ {
        let (id_a, id_b) = (TypeId::of::<A>(), TypeId::of::<B>());
        assert_ne!(id_a, id_b, "query2_mut needs two different component types");

        // Two disjoint '&mut' into the map; the std panics on equal key which
        // The assert above already turned into a clearer message.
        let [a, b] = self.columns.get_disjoint_mut([&id_a, &id_b]);
        let columns = a
            .and_then(|a| a.as_any().downcast_ref::<SparseSet<A>>())
            .zip(b.and_then(|b| b.as_any_mut().downcast_mut::<SparseSet<B>>()));

        columns.into_iter().flat_map(|(a, b)| {
            b.iter_mut()
                .filter_map(move |(entity, b)| Some((entity, a.get(entity)?, b)))
        })
    }

    /// How many entities carry a `T`.
    pub fn count<T: 'static>(&self) -> usize {
        self.columns
            .get(&TypeId::of::<T>())
            .map_or(0, |column| column.component_count())
    }

    /// The column of `T`, or `None` when no `T` was ever inserted.
    fn column_opt<T: 'static>(&self) -> Option<&SparseSet<T>> {
        self.columns
            .get(&TypeId::of::<T>())?
            .as_any()
            .downcast_ref::<SparseSet<T>>()
    }

    /// Mutable [`World::column_opt`].
    fn column_opt_mut<T: 'static>(&mut self) -> Option<&mut SparseSet<T>> {
        self.columns
            .get_mut(&TypeId::of::<T>())?
            .as_any_mut()
            .downcast_mut::<SparseSet<T>>()
    }

    /// The column of `T`, created empty when this is the first insert.
    fn column_mut<T: 'static>(&mut self) -> &mut SparseSet<T> {
        self.columns
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(SparseSet::<T>::new()))
            .as_any_mut()
            .downcast_mut::<SparseSet<T>>()
            // INVARIANT: the key is `TypeId::of::<T>()` and this method is the
            // only writer, and it always stores a `SparseSet<T>`. The downcast
            // cannot fail unless the map has been corrupted.
            .expect("a column keyed by TypeId::of::<T>() holds a SparseSet<T>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Componentes usados só aqui: dois tipos distintos e um sem campo.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Position(i32);
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Velocity(i32);

    /// Uma entidade nova não carrega nada até alguém anexar.
    #[test]
    fn a_fresh_entity_has_no_components() {
        let mut world = World::new();
        let e = world.spawn();

        assert!(world.is_alive(e));
        assert_eq!(world.get::<Position>(e), None);
        assert!(!world.contains::<Position>(e));
        assert_eq!(world.len(), 1);
    }

    /// A query só produz quem tem os dois componentes.
    #[test]
    fn query2_yields_only_the_intersection() {
        let mut world = World::new();
        let (both, only_pos, only_vel) = (world.spawn(), world.spawn(), world.spawn());
        world.insert(both, Position(1));
        world.insert(both, Velocity(10));
        world.insert(only_pos, Position(2));
        world.insert(only_vel, Velocity(30));

        let got: Vec<_> = world.query2::<Position, Velocity>().collect();

        assert_eq!(got, vec![(both, &Position(1), &Velocity(10))]);
    }

    /// `query2_mut` lê um lado e escreve no outro, sem tocar em quem não casa.
    #[test]
    fn query2_mut_writes_only_the_matching_entities() {
        let mut world = World::new();
        let (moving, still) = (world.spawn(), world.spawn());
        world.insert(moving, Position(1));
        world.insert(moving, Velocity(10));
        world.insert(still, Position(2));

        for (_, velocity, position) in world.query2_mut::<Velocity, Position>() {
            position.0 += velocity.0;
        }

        assert_eq!(world.get::<Position>(moving), Some(&Position(11)));
        assert_eq!(world.get::<Position>(still), Some(&Position(2)));
    }

    /// Tipo que nunca foi inserido: query vazia, sem pânico e sem criar coluna.
    #[test]
    fn a_query_over_an_unknown_component_is_empty() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(1));

        assert_eq!(world.query2::<Position, Velocity>().count(), 0);
        assert_eq!(world.query2_mut::<Velocity, Position>().count(), 0);
    }

    /// Emprestar a mesma coluna como `&` e `&mut` é o bug que a regra impede.
    #[test]
    #[should_panic(expected = "two different component types")]
    fn query2_mut_rejects_the_same_type_twice() {
        let mut world = World::new();
        let _ = world.query2_mut::<Position, Position>().count();
    }

    /// Colunas de tipos diferentes convivem sem se misturar.
    #[test]
    fn columns_of_different_types_do_not_collide() {
        let mut world = World::new();
        let e = world.spawn();

        assert!(world.insert(e, Position(1)));
        assert!(world.insert(e, Velocity(2)));

        assert_eq!(world.get::<Position>(e), Some(&Position(1)));
        assert_eq!(world.get::<Velocity>(e), Some(&Velocity(2)));
        assert_eq!(world.count::<Position>(), 1);
        assert_eq!(world.count::<Velocity>(), 1);
    }

    /// Entidades podem carregar conjuntos diferentes de componentes — o ponto
    /// inteiro de trocar o bundle único por colunas.
    #[test]
    fn entities_may_carry_different_component_sets() {
        let mut world = World::new();
        let (a, b) = (world.spawn(), world.spawn());
        world.insert(a, Position(1));
        world.insert(a, Velocity(1));
        world.insert(b, Position(2));

        assert_eq!(world.count::<Position>(), 2);
        assert_eq!(world.count::<Velocity>(), 1);
        assert!(!world.contains::<Velocity>(b));
    }

    /// Despawn varre todas as colunas — não pode sobrar componente órfão.
    #[test]
    fn despawn_clears_every_column() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(1));
        world.insert(e, Velocity(2));

        assert!(world.despawn(e));

        assert_eq!(world.count::<Position>(), 0);
        assert_eq!(world.count::<Velocity>(), 0);
        assert_eq!(world.get::<Position>(e), None);
        assert!(world.is_empty());
        assert!(!world.despawn(e), "despawn duplo é no-op");
    }

    /// Componente não sobrevive a uma entidade que já morreu.
    #[test]
    fn insert_on_a_stale_handle_is_refused() {
        let mut world = World::new();
        let e = world.spawn();
        world.despawn(e);

        assert!(!world.insert(e, Position(1)));
        assert_eq!(world.count::<Position>(), 0);
    }

    /// Reciclar o slot não deixa o handle antigo enxergar o novo inquilino.
    #[test]
    fn a_recycled_slot_does_not_leak_through_the_old_handle() {
        let mut world = World::new();
        let old = world.spawn();
        world.insert(old, Position(1));
        world.despawn(old);

        let new = world.spawn();
        assert_eq!(new.index(), old.index(), "o slot deve ser reciclado");
        world.insert(new, Position(2));

        assert_eq!(world.get::<Position>(new), Some(&Position(2)));
        assert_eq!(world.get::<Position>(old), None);
    }

    /// `remove` tira só o componente pedido; a entidade continua viva.
    #[test]
    fn remove_takes_one_component_and_leaves_the_entity() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(1));
        world.insert(e, Velocity(2));

        assert_eq!(world.remove::<Position>(e), Some(Position(1)));
        assert!(world.is_alive(e));
        assert_eq!(world.get::<Velocity>(e), Some(&Velocity(2)));
    }

    /// Consultar um tipo que nunca foi inserido não cria coluna nem entra em
    /// pânico.
    #[test]
    fn querying_an_unknown_component_is_empty() {
        let world = World::new();
        assert_eq!(world.count::<Position>(), 0);
        assert_eq!(world.iter::<Position>().count(), 0);
    }

    /// `iter_mut` escreve na coluna de verdade.
    #[test]
    fn iter_mut_writes_through() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(1));

        for (_, position) in world.iter_mut::<Position>() {
            position.0 += 10;
        }

        assert_eq!(world.get::<Position>(e), Some(&Position(11)));
    }

    /// O padrão que os sistemas usam até a etapa 07: iterar uma coluna e
    /// buscar a outra por entidade.
    #[test]
    fn iterating_one_column_and_looking_up_another() {
        let mut world = World::new();
        let (a, b) = (world.spawn(), world.spawn());
        world.insert(a, Position(1));
        world.insert(a, Velocity(10));
        world.insert(b, Position(2)); // sem Velocity: fica de fora

        let moved: Vec<i32> = world
            .iter::<Velocity>()
            .filter_map(|(e, v)| world.get::<Position>(e).map(|p| p.0 + v.0))
            .collect();

        assert_eq!(moved, vec![11]);
    }

    /// `single` find the only entity with label.
    #[test]
    fn single_finds_the_one_entity() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Position(1));

        assert_eq!(world.single::<Position>(), &Position(1));
    }

    /// If no entity with label: breaks with clear message.
    #[test]
    #[should_panic(expected = "found no entity")]
    fn single_panics_when_nobody_has_it() {
        World::new().single::<Position>();
    }

    /// two entities with the same label: Also a dev error.
    #[test]
    #[should_panic(expected = "found more than one")]
    fn single_panics_when_more_than_one_has_it() {
        let mut world = World::new();
        let (a, b) = (world.spawn(), world.spawn());
        world.insert(a, Position(1));
        world.insert(b, Position(2));

        world.single::<Position>();
    }
}