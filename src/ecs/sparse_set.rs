//! Sparse-set component storage: the column that holds one component type.
//!
//! The structure trades memory for speed in the way an ECS wants. A big,
//! hole-ridden index array (`sparse`) buys O(1) lookup by entity, while the
//! components themselves live packed in `dense`, so iterating a whole column
//! is a linear walk over contiguous memory instead of a hunt through
//! `Option`s.
//!
//! ```text
//!   entities with a component: e0, e2, e5
//!
//!   sparse   [Some(0), None, Some(1), None, None, Some(2)]
//!             e0             e2                   e5        (index = entity)
//!   dense    [C0, C2, C5]        packed, no holes — iterate this
//!   entities [e0, e2, e5]        who owns each dense slot
//! ```
//!
//! Removal is a `swap_remove`: the last element fills the hole, which keeps
//! `dense` packed in O(1) at the cost of the ordering — nothing may depend on
//! the order components come out in.

use crate::ecs::Entity;

/// Every component of one type `T`, keyed by the entity that owns it.
pub struct SparseSet<T> {
    /// The components, packed with no holes — this is what systems iterate.
    dense: Vec<T>,
    /// Which entity owns `dense[i]`. Parallel to `dense`, same length.
    ///
    /// Storing the whole [`Entity`] and not just its index is what lets
    /// lookups reject a stale handle without consulting the allocator.
    entities: Vec<Entity>,
    /// Indexed by [`Entity::index`]: where that entity's component sits in
    /// `dense`, or `None` when it has none.
    sparse: Vec<Option<u32>>,
}

// Written by hand because `#[derive(Default)]` would demand `T: Default`,
// which no component should have to implement just to be storable.
impl<T> Default for SparseSet<T> {
    fn default() -> Self {
        Self {
            dense: Vec::new(),
            entities: Vec::new(),
            sparse: Vec::new(),
        }
    }
}

impl<T> SparseSet<T> {
    /// An empty column.
    pub fn new() -> Self {
        Self::default()
    }

    /// Attaches `value` to `entity`, returning the component it replaced.
    pub fn insert(&mut self, entity: Entity, value: T) -> Option<T> {
        let slot = entity.index() as usize;
        if slot >= self.sparse.len() {
            self.sparse.resize(slot + 1, None);
        }

        if let Some(dense_index) = self.sparse[slot] {
            let dense_index = dense_index as usize;
            // Refresh the stored handle: the slot may have been recycled since
            // the last insert, and the newer generation is the one that has to
            // match from now on.
            self.entities[dense_index] = entity;
            return Some(std::mem::replace(&mut self.dense[dense_index], value));
        }

        self.sparse[slot] = Some(self.dense.len() as u32);
        self.dense.push(value);
        self.entities.push(entity);
        None
    }

    /// Detaches and returns the component of `entity`, if it has one.
    pub fn remove(&mut self, entity: Entity) -> Option<T> {
        let dense_index = self.dense_index(entity)?;

        self.sparse[entity.index() as usize] = None;
        let value = self.dense.swap_remove(dense_index);
        self.entities.swap_remove(dense_index);

        // `swap_remove` moved the last element into the freed position — unless
        // the freed position *was* the last. Whoever moved needs its sparse
        // entry repointed at the new home, or every later lookup for it reads
        // somebody else's component.
        if dense_index < self.dense.len() {
            let moved = self.entities[dense_index];
            self.sparse[moved.index() as usize] = Some(dense_index as u32);
        }

        Some(value)
    }

    /// Borrows the component of `entity`, or `None` when it has none or the
    /// handle is stale.
    pub fn get(&self, entity: Entity) -> Option<&T> {
        self.dense_index(entity).map(|i| &self.dense[i])
    }

    /// Mutably borrows the component of `entity`.
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        self.dense_index(entity).map(|i| &mut self.dense[i])
    }

    /// Whether `entity` currently has a component in this column.
    pub fn contains(&self, entity: Entity) -> bool {
        self.dense_index(entity).is_some()
    }

    /// Iterates `(entity, component)` in dense order.
    ///
    /// Dense order is insertion order only until the first [`SparseSet::remove`]
    /// — the `swap_remove` shuffles it. Nothing may depend on it.
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &T)> + '_ {
        self.entities.iter().copied().zip(self.dense.iter())
    }

    /// Mutable counterpart of [`SparseSet::iter`].
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut T)> + '_ {
        self.entities.iter().copied().zip(self.dense.iter_mut())
    }

    /// The packed components, for a system that does not need the ids.
    pub fn values(&self) -> &[T] {
        &self.dense
    }

    /// How many entities have this component.
    pub fn len(&self) -> usize {
        self.dense.len()
    }

    /// Whether no entity has this component.
    pub fn is_empty(&self) -> bool {
        self.dense.is_empty()
    }

    /// Resolves an entity to its position in `dense`, rejecting stale handles.
    ///
    /// The generation comparison is what stops a recycled slot from handing an
    /// old handle the new tenant's component; `entities` already holds the
    /// data, so the check costs one comparison and no extra indirection.
    fn dense_index(&self, entity: Entity) -> Option<usize> {
        let dense_index = self
            .sparse
            .get(entity.index() as usize)
            .copied()
            .flatten()? as usize;

        (self.entities[dense_index] == entity).then_some(dense_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::EntityAllocator;

    /// O ciclo básico: anexa, lê, remove, sumiu.
    #[test]
    fn insert_get_remove() {
        let mut allocator = EntityAllocator::new();
        let e = allocator.spawn();
        let mut set = SparseSet::<i32>::new();

        assert_eq!(set.insert(e, 42), None);
        assert_eq!(set.get(e), Some(&42));
        assert!(set.contains(e));
        assert_eq!(set.remove(e), Some(42));
        assert_eq!(set.get(e), None);
        assert!(set.is_empty());
    }

    /// Inserir de novo substitui em vez de duplicar a linha no dense.
    #[test]
    fn insert_over_an_existing_component_replaces_it() {
        let mut allocator = EntityAllocator::new();
        let e = allocator.spawn();
        let mut set = SparseSet::<i32>::new();

        set.insert(e, 1);
        assert_eq!(set.insert(e, 2), Some(1));
        assert_eq!(set.len(), 1);
        assert_eq!(set.get(e), Some(&2));
    }

    /// Remover quem não está na coluna é inofensivo.
    #[test]
    fn removing_an_absent_component_is_a_no_op() {
        let mut allocator = EntityAllocator::new();
        let (a, b) = (allocator.spawn(), allocator.spawn());
        let mut set = SparseSet::new();
        set.insert(a, 'a');

        assert_eq!(set.remove(b), None);
        assert_eq!(set.len(), 1);
        assert_eq!(set.get(a), Some(&'a'));
    }

    /// O caso que o `swap_remove` corrompe se ninguém arrumar o sparse:
    /// remover do meio move o último para o buraco.
    #[test]
    fn swap_remove_fixes_up_the_moved_entity() {
        let mut allocator = EntityAllocator::new();
        let (a, b, c) = (allocator.spawn(), allocator.spawn(), allocator.spawn());
        let mut set = SparseSet::new();
        set.insert(a, 'a');
        set.insert(b, 'b');
        set.insert(c, 'c');

        assert_eq!(set.remove(b), Some('b'));

        assert_eq!(set.get(a), Some(&'a'));
        assert_eq!(
            set.get(c),
            Some(&'c'),
            "c foi movido para o buraco; o sparse tem que segui-lo"
        );
        assert_eq!(set.len(), 2);
    }

    /// Remover o último não move ninguém — o caminho sem fix-up.
    #[test]
    fn removing_the_last_element_touches_nobody_else() {
        let mut allocator = EntityAllocator::new();
        let (a, b) = (allocator.spawn(), allocator.spawn());
        let mut set = SparseSet::new();
        set.insert(a, 'a');
        set.insert(b, 'b');

        assert_eq!(set.remove(b), Some('b'));
        assert_eq!(set.get(a), Some(&'a'));
    }

    /// Handle velho de slot reciclado não pode ler o componente do novo dono.
    #[test]
    fn a_stale_handle_does_not_read_the_new_tenant() {
        let mut allocator = EntityAllocator::new();
        let old = allocator.spawn();
        let mut set = SparseSet::new();
        set.insert(old, 1);

        allocator.despawn(old);
        let new = allocator.spawn();
        assert_eq!(new.index(), old.index(), "o slot deve ser reciclado");
        set.insert(new, 2);

        assert_eq!(set.get(new), Some(&2));
        assert_eq!(set.get(old), None, "a geração antiga não casa mais");
        assert!(!set.contains(old));
    }

    /// `iter` vê exatamente o que está no dense, com o dono certo.
    #[test]
    fn iter_pairs_each_component_with_its_entity() {
        let mut allocator = EntityAllocator::new();
        let (a, b) = (allocator.spawn(), allocator.spawn());
        let mut set = SparseSet::new();
        set.insert(a, 10);
        set.insert(b, 20);

        let mut pairs: Vec<_> = set.iter().map(|(e, v)| (e, *v)).collect();
        pairs.sort_by_key(|(e, _)| e.index());
        assert_eq!(pairs, vec![(a, 10), (b, 20)]);
    }

    /// `iter_mut` escreve no dense de verdade.
    #[test]
    fn iter_mut_writes_through() {
        let mut allocator = EntityAllocator::new();
        let e = allocator.spawn();
        let mut set = SparseSet::new();
        set.insert(e, 1);

        for (_, value) in set.iter_mut() {
            *value += 1;
        }

        assert_eq!(set.get(e), Some(&2));
    }

    /// Os três arrays têm que continuar consistentes depois de qualquer
    /// sequência de operações — inclusive as que um teste manual não imagina.
    /// A referência é um `HashMap`: obviamente correto, obviamente lento.
    #[test]
    fn fuzz_matches_a_reference_hashmap() {
        use std::collections::HashMap;

        let mut allocator = EntityAllocator::new();
        let entities: Vec<_> = (0..64).map(|_| allocator.spawn()).collect();

        let mut set = SparseSet::<u32>::new();
        let mut reference: HashMap<Entity, u32> = HashMap::new();

        // xorshift com semente fixa: reproduzir uma falha não depende de sorte.
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for step in 0..10_000u32 {
            let e = entities[(next() % entities.len() as u64) as usize];

            if next() % 3 == 0 {
                assert_eq!(set.remove(e), reference.remove(&e), "remove divergiu");
            } else {
                assert_eq!(set.insert(e, step), reference.insert(e, step), "insert");
            }

            assert_eq!(set.len(), reference.len(), "dense e referência divergiram");
        }

        for &e in &entities {
            assert_eq!(set.get(e), reference.get(&e), "leitura divergente");
        }
    }
}
