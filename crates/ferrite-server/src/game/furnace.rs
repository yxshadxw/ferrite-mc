//! Furnace-family block entities (vanilla `AbstractFurnaceBlockEntity`).
//!
//! The three input/fuel/result stacks and the burn and cook timers live in the block entity's
//! NBT, like vanilla's `Items`, `BurnTime`, `CookTime`, `CookTimeTotal` and the accumulated
//! experience. Opening a furnace copies that state into its menu; closing writes it back.
//! Like vanilla, a menu only ticks while a player has it open in this implementation.

use ferrite_inventory::{FurnaceKind, FurnaceState};
use ferrite_item::ItemStack;
use ferrite_nbt::{Compound, List, Tag};
use ferrite_world::BlockEntity;

use super::anvil::AnvilChunkSource;

/// Slots of one furnace: input, fuel, result.
pub const SLOTS: usize = 3;

/// The block entity state of a furnace, blast furnace or smoker.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FurnaceData {
    pub items: Vec<ItemStack>,
    pub state: FurnaceState,
}

/// The furnace family block at a name, if it is one.
pub fn kind(name: &str) -> Option<FurnaceKind> {
    Some(match name {
        "minecraft:furnace" => FurnaceKind::Furnace,
        "minecraft:blast_furnace" => FurnaceKind::BlastFurnace,
        "minecraft:smoker" => FurnaceKind::Smoker,
        _ => return None,
    })
}

fn number(entity: Option<&BlockEntity>, key: &str) -> i32 {
    match entity.and_then(|entity| entity.data.get(key)) {
        Some(Tag::Short(value)) => i32::from(*value),
        Some(Tag::Int(value)) => *value,
        _ => 0,
    }
}

/// Vanilla `ContainerHelper.loadAllItems` for the three furnace slots.
fn load_items(entity: Option<&BlockEntity>) -> Vec<ItemStack> {
    let mut items = vec![ItemStack::empty(); SLOTS];
    let Some(Tag::List(List::Compound(list))) = entity.and_then(|e| e.data.get("Items")) else {
        return items;
    };
    for entry in list {
        let Some(Tag::Byte(slot)) = entry.get("Slot") else {
            continue;
        };
        let Some(target) = usize::try_from(*slot).ok().and_then(|s| items.get_mut(s)) else {
            continue;
        };
        match ItemStack::from_nbt(entry) {
            Ok(stack) => *target = stack,
            Err(error) => tracing::warn!(%error, "ignoring an unreadable furnace item"),
        }
    }
    items
}

/// Vanilla `ContainerHelper.saveAllItems`.
fn save_items(items: &[ItemStack]) -> Tag {
    let list = items
        .iter()
        .enumerate()
        .filter(|(_, stack)| !stack.is_empty())
        .filter_map(|(slot, stack)| {
            let mut entry = stack.to_nbt().ok()?;
            entry.insert("Slot".into(), Tag::Byte(slot as i8));
            Some(entry)
        })
        .collect();
    Tag::List(List::Compound(list))
}

/// Reads a furnace's items and timers.
pub fn load(source: &AnvilChunkSource, [x, y, z]: [i32; 3]) -> FurnaceData {
    let entity = source.block_entity(x, y, z);
    let entity = entity.as_ref();
    FurnaceData {
        items: load_items(entity),
        state: FurnaceState {
            cook: number(entity, "CookTime"),
            cook_total: number(entity, "CookTimeTotal"),
            burn: number(entity, "BurnTime"),
            burn_total: number(entity, "BurnTimeTotal"),
            pending_experience: match entity.and_then(|e| e.data.get("Experience")) {
                Some(Tag::Float(value)) => *value,
                Some(Tag::Double(value)) => *value as f32,
                _ => 0.0,
            },
        },
    }
}

/// Writes the items and timers back, creating the block entity when needed.
pub fn store(
    source: &AnvilChunkSource,
    [x, y, z]: [i32; 3],
    kind: FurnaceKind,
    data: &FurnaceData,
) {
    let Some(type_id) = ferrite_data::registries::id("minecraft:block_entity_type", kind.block())
        .and_then(|id| u32::try_from(id).ok())
    else {
        return;
    };
    let result = source.update_block_entity(x, y, z, |entity| {
        let current = entity.as_ref().map(|entity| FurnaceData {
            items: load_items(Some(entity)),
            state: FurnaceState {
                cook: number(Some(entity), "CookTime"),
                cook_total: number(Some(entity), "CookTimeTotal"),
                burn: number(Some(entity), "BurnTime"),
                burn_total: number(Some(entity), "BurnTimeTotal"),
                pending_experience: match entity.data.get("Experience") {
                    Some(Tag::Float(value)) => *value,
                    _ => 0.0,
                },
            },
        });
        if current.as_ref() == Some(data) {
            return (false, ());
        }
        let entity = entity.get_or_insert_with(|| {
            let mut nbt = Compound::new();
            nbt.insert("id".into(), Tag::String(kind.block().into()));
            nbt.insert("x".into(), Tag::Int(x));
            nbt.insert("y".into(), Tag::Int(y));
            nbt.insert("z".into(), Tag::Int(z));
            BlockEntity { type_id, data: nbt }
        });
        entity.data.insert("Items".into(), save_items(&data.items));
        entity
            .data
            .insert("BurnTime".into(), Tag::Short(data.state.burn as i16));
        entity.data.insert(
            "BurnTimeTotal".into(),
            Tag::Short(data.state.burn_total as i16),
        );
        entity
            .data
            .insert("CookTime".into(), Tag::Short(data.state.cook as i16));
        entity.data.insert(
            "CookTimeTotal".into(),
            Tag::Short(data.state.cook_total as i16),
        );
        entity.data.insert(
            "Experience".into(),
            Tag::Float(data.state.pending_experience),
        );
        (true, ())
    });
    if let Err(error) = result {
        tracing::warn!(x, y, z, %error, "could not store furnace state");
    }
}

/// Removes the block entity of a broken furnace and returns its items (vanilla
/// `Containers.dropContentsOnDestroy`).
pub fn take_contents(source: &AnvilChunkSource, [x, y, z]: [i32; 3]) -> Vec<ItemStack> {
    source
        .update_block_entity(x, y, z, |entity| {
            entity.take().map_or((false, Vec::new()), |removed| {
                (true, load_items(Some(&removed)))
            })
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|stack| !stack.is_empty())
        .collect()
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
    async fn furnace_items_and_timers_round_trip_through_the_block_entity() {
        let source = loaded().await;
        let state = u32::from(
            ferrite_data::blocks::state_id("furnace", &[("lit", "false"), ("facing", "north")])
                .unwrap(),
        );
        source.set_block(4, 64, 4, state).unwrap();
        let mut data = FurnaceData {
            items: vec![ItemStack::empty(); SLOTS],
            state: FurnaceState {
                cook: 117,
                cook_total: 200,
                burn: 400,
                burn_total: 1600,
                pending_experience: 1.5,
            },
        };
        data.items[0] = ItemStack::from_name("raw_gold").unwrap();
        data.items[1] = ItemStack::from_name("coal").unwrap().copy_with_count(3);
        store(&source, [4, 64, 4], FurnaceKind::Furnace, &data);
        assert_eq!(load(&source, [4, 64, 4]), data);
        // Storing unchanged state does not mark the chunk modified again.
        store(&source, [4, 64, 4], FurnaceKind::Furnace, &data);
        let spilled = take_contents(&source, [4, 64, 4]);
        assert_eq!(spilled, vec![data.items[0].clone(), data.items[1].clone()]);
    }
}
