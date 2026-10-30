//! Entity identifiers: network ids, UUIDs and region-local store keys.

use std::sync::atomic::{AtomicI32, Ordering};
use uuid::Uuid;

/// Process-wide network id counter, like vanilla's static `Entity.ENTITY_COUNTER`.
///
/// Ids are never reused while the process runs. Players draw from the same counter so no two
/// entities, players included, ever share an id.
static NEXT_ID: AtomicI32 = AtomicI32::new(1);

/// Allocates a fresh network entity id.
pub fn allocate_id() -> i32 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// A random (version 4) UUID for a newly created entity.
pub fn random_uuid() -> Uuid {
    Uuid::new_v4()
}

/// Key of an entity inside one [`crate::EntityStore`].
///
/// Keys are generational: a key of a removed entity never resolves to a later entity that
/// reuses the slot. They are local to the store that issued them and are invalidated when the
/// entity leaves it (removal, region merge or split, or a hand-off to another region); the
/// network id and the UUID are the stable identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityKey {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_increasing() {
        let a = allocate_id();
        let b = allocate_id();
        assert!(b > a);
        assert_ne!(random_uuid(), random_uuid());
    }
}
