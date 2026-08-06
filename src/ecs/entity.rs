//! Generational entity identifiers — the foundation of the ECS.
//!
//! An entity is *just an id*. It owns no data and no behaviour; components
//! live in storages keyed by the entity's index. The problem that shape
//! creates is the **dangling id**: code holds an `Entity`, the entity is
//! despawned, the slot is reused by a new entity, and the stale handle now
//! silently points at somebody else's data.
//!
//! The fix is a generation counter per slot. Reusing a slot bumps its
//! generation, so an old handle no longer matches and
//! [`EntityAllocator::is_alive`] returns `false` instead of aliasing.

/// A handle to an entity: a slot index plus the generation that occupied it.
///
/// Cheap to copy (8 bytes) and meaningful only against the
/// [`EntityAllocator`] that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity {
    /// Dense slot index — also the index into every component storage.
    index: u32,
    /// How many times this slot had been reused when the handle was minted.
    generation: u32,
}

impl Entity {
    /// The slot index, used to address component storages.
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// The generation this handle was minted with.
    pub const fn generation(&self) -> u32 {
        self.generation
    }
}

/// Allocates and recycles [`Entity`] handles.
///
/// Slots are recycled last-in-first-out so live entities stay packed near the
/// start of the array, which keeps component storages dense.
#[derive(Debug, Clone, Default)]
pub struct EntityAllocator {
    /// Current generation of every slot ever allocated, indexed by slot.
    generations: Vec<u32>,
    /// Whether each slot currently holds a live entity.
    alive: Vec<bool>,
    /// Slots freed by `despawn`, ready to be handed out again.
    free: Vec<u32>,
    /// Number of live entities — tracked to keep [`EntityAllocator::len`] O(1).
    live_count: usize,
}

impl EntityAllocator {
    /// Creates an empty allocator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an allocator with room for `capacity` slots, avoiding
    /// reallocation while the scene is being populated.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            generations: Vec::with_capacity(capacity),
            alive: Vec::with_capacity(capacity),
            free: Vec::new(),
            live_count: 0,
        }
    }

    /// Mints a new entity, reusing a freed slot when one is available.
    pub fn spawn(&mut self) -> Entity {
        self.live_count += 1;

        if let Some(index) = self.free.pop() {
            let slot = index as usize;
            self.alive[slot] = true;
            return Entity {
                index,
                generation: self.generations[slot],
            };
        }

        let index = self.generations.len() as u32;
        self.generations.push(0);
        self.alive.push(true);
        Entity {
            index,
            generation: 0,
        }
    }

    /// Retires an entity and queues its slot for reuse.
    ///
    /// Returns `false` — and changes nothing — when the handle is stale or was
    /// never minted here, which makes double-despawn harmless.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.is_alive(entity) {
            return false;
        }

        let slot = entity.index as usize;
        self.alive[slot] = false;
        // Bumping the generation is what invalidates every outstanding handle
        // to this slot before it can be handed to a different entity.
        self.generations[slot] = self.generations[slot].wrapping_add(1);
        self.free.push(entity.index);
        self.live_count -= 1;
        true
    }

    /// Whether the handle still refers to the entity it was minted for.
    pub fn is_alive(&self, entity: Entity) -> bool {
        let slot = entity.index as usize;
        slot < self.generations.len()
            && self.alive[slot]
            && self.generations[slot] == entity.generation
    }

    /// Number of live entities.
    pub fn len(&self) -> usize {
        self.live_count
    }

    /// Whether no entity is currently alive.
    pub fn is_empty(&self) -> bool {
        self.live_count == 0
    }

    /// Highest slot index ever allocated, plus one.
    ///
    /// Component storages are sized against this, not against
    /// [`EntityAllocator::len`], because freed slots leave holes.
    pub fn capacity(&self) -> usize {
        self.generations.len()
    }

    /// Iterates over the live entities in slot order.
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.alive
            .iter()
            .enumerate()
            .filter(|&(_, &alive)| alive)
            .map(|(slot, _)| Entity {
                index: slot as u32,
                generation: self.generations[slot],
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Slots são entregues em ordem enquanto não houver nada para reciclar.
    #[test]
    fn spawn_hands_out_fresh_slots() {
        let mut allocator = EntityAllocator::new();
        let a = allocator.spawn();
        let b = allocator.spawn();

        assert_eq!((a.index(), a.generation()), (0, 0));
        assert_eq!((b.index(), b.generation()), (1, 0));
        assert_eq!(allocator.len(), 2);
    }

    /// O caso que justifica a geração: o slot volta, o handle antigo não.
    #[test]
    fn recycled_slot_invalidates_the_old_handle() {
        let mut allocator = EntityAllocator::new();
        let old = allocator.spawn();
        allocator.despawn(old);

        let new = allocator.spawn();
        assert_eq!(new.index(), old.index(), "o slot deve ser reaproveitado");
        assert_ne!(new.generation(), old.generation());
        assert!(!allocator.is_alive(old));
        assert!(allocator.is_alive(new));
    }

    /// Despawn duplicado é inofensivo e não devolve o slot duas vezes.
    #[test]
    fn double_despawn_is_a_no_op() {
        let mut allocator = EntityAllocator::new();
        let e = allocator.spawn();

        assert!(allocator.despawn(e));
        assert!(!allocator.despawn(e));
        assert_eq!(allocator.len(), 0);
        assert_eq!(allocator.capacity(), 1, "nenhum slot novo foi criado");
    }

    /// `iter` enxerga só quem está vivo, na ordem dos slots.
    #[test]
    fn iter_skips_dead_entities() {
        let mut allocator = EntityAllocator::new();
        let a = allocator.spawn();
        let b = allocator.spawn();
        let c = allocator.spawn();
        allocator.despawn(b);

        assert_eq!(allocator.iter().collect::<Vec<_>>(), vec![a, c]);
    }

    /// Um handle de outro allocator não pode passar por vivo.
    #[test]
    fn foreign_handles_are_not_alive() {
        let allocator = EntityAllocator::new();
        let mut other = EntityAllocator::new();
        let foreign = other.spawn();

        assert!(!allocator.is_alive(foreign));
        assert!(allocator.is_empty());
    }
}
