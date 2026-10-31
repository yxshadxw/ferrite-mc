//! Beacon block entities (vanilla `BeaconBlockEntity`).
//!
//! A beacon stores its pyramid level, the chosen primary and secondary effects and the payment
//! in its block entity NBT (`Levels`, `primary_effect`, `secondary_effect`, `Payment`). There is
//! no beacon UI or effect application yet, so this module only owns that saved state: it reads
//! and writes the vanilla keys so a beacon placed and configured later survives a restart.

use ferrite_nbt::{Compound, Tag};
use ferrite_world::BlockEntity;

use super::anvil::AnvilChunkSource;

/// The block entity type name of a beacon.
pub const BLOCK: &str = "minecraft:beacon";

/// The saved state of a beacon.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeaconData {
    /// `Levels`: pyramid levels (0 to 4).
    pub levels: i32,
    /// `primary_effect`, e.g. `minecraft:speed`.
    pub primary_effect: Option<String>,
    /// `secondary_effect`, the second effect at level 4.
    pub secondary_effect: Option<String>,
    /// `Payment`: the item id consumed to change the effects.
    pub payment: i32,
}

fn effect(entity: Option<&BlockEntity>, key: &str) -> Option<String> {
    match entity.and_then(|entity| entity.data.get(key)) {
        Some(Tag::String(name)) if !name.is_empty() => Some(name.clone()),
        _ => None,
    }
}

fn number(entity: Option<&BlockEntity>, key: &str) -> i32 {
    match entity.and_then(|entity| entity.data.get(key)) {
        Some(Tag::Int(value)) => *value,
        Some(Tag::Short(value)) => i32::from(*value),
        _ => 0,
    }
}

/// Reads a beacon's saved state; a missing block entity reads as a fresh beacon.
pub fn load(source: &AnvilChunkSource, [x, y, z]: [i32; 3]) -> BeaconData {
    let entity = source.block_entity(x, y, z);
    let entity = entity.as_ref();
    BeaconData {
        levels: number(entity, "Levels"),
        primary_effect: effect(entity, "primary_effect"),
        secondary_effect: effect(entity, "secondary_effect"),
        payment: number(entity, "Payment"),
    }
}

/// Writes a beacon's state back, creating the block entity when one is missing.
pub fn store(source: &AnvilChunkSource, [x, y, z]: [i32; 3], data: &BeaconData) {
    let Some(type_id) = ferrite_data::registries::id("minecraft:block_entity_type", BLOCK)
        .and_then(|id| u32::try_from(id).ok())
    else {
        return;
    };
    let result = source.update_block_entity(x, y, z, |entity| {
        let current = entity.as_ref().map(|entity| BeaconData {
            levels: number(Some(entity), "Levels"),
            primary_effect: effect(Some(entity), "primary_effect"),
            secondary_effect: effect(Some(entity), "secondary_effect"),
            payment: number(Some(entity), "Payment"),
        });
        if current.as_ref() == Some(data) {
            return (false, ());
        }
        let entity = entity.get_or_insert_with(|| {
            let mut nbt = Compound::new();
            nbt.insert("id".into(), Tag::String(BLOCK.into()));
            nbt.insert("x".into(), Tag::Int(x));
            nbt.insert("y".into(), Tag::Int(y));
            nbt.insert("z".into(), Tag::Int(z));
            BlockEntity { type_id, data: nbt }
        });
        entity.data.insert("Levels".into(), Tag::Int(data.levels));
        match &data.primary_effect {
            Some(effect) => {
                entity
                    .data
                    .insert("primary_effect".into(), Tag::String(effect.clone()));
            }
            None => {
                entity.data.shift_remove("primary_effect");
            }
        }
        match &data.secondary_effect {
            Some(effect) => {
                entity
                    .data
                    .insert("secondary_effect".into(), Tag::String(effect.clone()));
            }
            None => {
                entity.data.shift_remove("secondary_effect");
            }
        }
        entity.data.insert("Payment".into(), Tag::Int(data.payment));
        (true, ())
    });
    if let Err(error) = result {
        tracing::warn!(x, y, z, %error, "could not store beacon state");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn loaded() -> std::sync::Arc<AnvilChunkSource> {
        let source = AnvilChunkSource::new_memory();
        while source.get(0, 0, 0).is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        source
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn beacon_state_round_trips_through_the_block_entity() {
        let source = loaded().await;
        let state =
            u32::from(ferrite_data::blocks::state_id("beacon", &[]).expect("vanilla has a beacon"));
        source.set_block(6, 70, 6, state).unwrap();
        assert_eq!(load(&source, [6, 70, 6]), BeaconData::default());
        let data = BeaconData {
            levels: 3,
            primary_effect: Some("minecraft:speed".into()),
            secondary_effect: None,
            payment: 1,
        };
        store(&source, [6, 70, 6], &data);
        assert_eq!(load(&source, [6, 70, 6]), data);
        let entity = source.block_entity(6, 70, 6).expect("beacon block entity");
        assert_eq!(
            entity.data.get("id"),
            Some(&Tag::String("minecraft:beacon".into()))
        );
        assert_eq!(entity.data.get("Levels"), Some(&Tag::Int(3)));
        // Clearing an effect removes the key instead of writing an empty one.
        let cleared = BeaconData {
            levels: 1,
            primary_effect: None,
            secondary_effect: None,
            payment: 0,
        };
        store(&source, [6, 70, 6], &cleared);
        assert_eq!(load(&source, [6, 70, 6]), cleared);
        assert!(
            source
                .block_entity(6, 70, 6)
                .unwrap()
                .data
                .get("primary_effect")
                .is_none()
        );
    }
}
