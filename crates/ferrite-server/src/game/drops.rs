//! Block drops: vanilla loot tables rolled when a survival player breaks a block.

use ferrite_loot::{LootContext, LootTables};
use ferrite_protocol::packets::play::ItemStack;
use std::sync::OnceLock;

/// Loot tables from the extracted vanilla data, loaded on first use. `None` (no data on this
/// machine, or a load error) means blocks drop nothing.
fn tables() -> Option<&'static LootTables> {
    static TABLES: OnceLock<Option<LootTables>> = OnceLock::new();
    TABLES
        .get_or_init(|| {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../vanilla/26.3/generated/data/minecraft/loot_table");
            LootTables::load(&dir)
                .inspect_err(|error| tracing::warn!(%error, "block drops disabled: no loot tables"))
                .ok()
        })
        .as_ref()
}

/// Loads the loot tables off the tick threads, so the first broken block does not stall its
/// region while they parse.
pub fn preload() {
    std::thread::spawn(|| {
        tables();
    });
}

/// Adapts the entity RNG to loot evaluation.
struct LootRng<'a>(&'a mut ferrite_entity::Rng);

impl ferrite_loot::Rng for LootRng<'_> {
    fn next_int(&mut self, bound: i32) -> i32 {
        self.0.next_int(bound)
    }

    fn next_float(&mut self) -> f32 {
        self.0.next_f32()
    }
}

/// Vanilla `Tool.isCorrectForDrops`: the first rule that names the block and decides drops.
fn tool_is_correct(item_id: i32, block: &str) -> bool {
    let Some(defaults) = u16::try_from(item_id)
        .ok()
        .and_then(ferrite_item::item_defaults)
    else {
        return false;
    };
    let tool_id = ferrite_item::component_ids::TOOL;
    let Some(tag) = defaults.components().remove(&tool_id) else {
        return false;
    };
    let ferrite_item::Component::Tool(tool) = ferrite_item::Component::from_nbt(tool_id, &tag)
    else {
        return false;
    };
    tool.rules
        .iter()
        .filter_map(|rule| rule.correct_for_drops.map(|correct| (rule, correct)))
        .find(|(rule, _)| match rule.blocks.strip_prefix('#') {
            Some(tag) => ferrite_data::tags::contains("block", tag, block),
            None => rule.blocks.split(',').any(|b| {
                b.trim().strip_prefix("minecraft:").unwrap_or(b.trim())
                    == block.strip_prefix("minecraft:").unwrap_or(block)
            }),
        })
        .is_some_and(|(_, correct)| correct)
}

/// What breaking `state` with `tool` drops (vanilla `Block.getDrops`, only when the player
/// `hasCorrectToolForDrops`).
pub fn block_drops(state: u16, tool: ItemStack, rng: &mut ferrite_entity::Rng) -> Vec<ItemStack> {
    if usize::from(state) >= ferrite_data::blocks::STATE_COUNT {
        return Vec::new();
    }
    let Some((name, props)) = ferrite_data::blocks::state(state) else {
        return Vec::new();
    };
    if ferrite_data::block_props::get(state).requires_correct_tool
        && (tool.count <= 0 || !tool_is_correct(tool.item_id, name))
    {
        return Vec::new();
    }
    let Some(table) = tables().and_then(|t| {
        t.get(&format!(
            "minecraft:blocks/{}",
            name.strip_prefix("minecraft:").unwrap_or(name)
        ))
    }) else {
        return Vec::new();
    };
    let context = LootContext {
        tool: (tool.count > 0)
            .then(|| u16::try_from(tool.item_id).ok())
            .flatten()
            .and_then(ferrite_item::item_name)
            .map(str::to_owned),
        block: Some(name.to_owned()),
        block_state: props
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        ..LootContext::default()
    };
    table
        .roll(&context, &mut LootRng(rng))
        .into_iter()
        .filter_map(|(item, count)| {
            let item_id = ferrite_data::registries::id("minecraft:item", &item)?;
            (count > 0).then_some(ItemStack { count, item_id })
        })
        .collect()
}

/// Rolls a mob's entity loot table (vanilla `LivingEntity.dropFromLootTable`), for example
/// leather and beef for a cow.
pub fn mob_drops(
    kind: ferrite_entity::mob::MobKind,
    rng: &mut ferrite_entity::Rng,
) -> Vec<ItemStack> {
    let Some(table) = tables().and_then(|tables| tables.get(kind.loot_table())) else {
        return Vec::new();
    };
    let context = LootContext {
        killed_by_player: true,
        ..LootContext::default()
    };
    table
        .roll(&context, &mut LootRng(rng))
        .into_iter()
        .filter_map(|(item, count)| {
            let item_id = ferrite_data::registries::id("minecraft:item", &item)?;
            (count > 0).then_some(ItemStack { count, item_id })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str) -> ItemStack {
        ItemStack {
            count: 1,
            item_id: ferrite_data::registries::id("minecraft:item", name).unwrap(),
        }
    }

    #[test]
    fn blocks_drop_their_loot_with_the_right_tool() {
        let mut rng = ferrite_entity::Rng::new(4);
        let grass = ferrite_data::blocks::default_state("grass_block").unwrap();
        assert_eq!(
            block_drops(grass, ItemStack::default(), &mut rng),
            [item("minecraft:dirt")]
        );
        let stone = ferrite_data::blocks::default_state("stone").unwrap();
        // Stone needs a pickaxe.
        assert!(block_drops(stone, ItemStack::default(), &mut rng).is_empty());
        assert!(block_drops(stone, item("minecraft:wooden_shovel"), &mut rng).is_empty());
        assert_eq!(
            block_drops(stone, item("minecraft:wooden_pickaxe"), &mut rng),
            [item("minecraft:cobblestone")]
        );
        // Iron ore needs at least a stone pickaxe.
        let ore = ferrite_data::blocks::default_state("iron_ore").unwrap();
        assert!(block_drops(ore, item("minecraft:wooden_pickaxe"), &mut rng).is_empty());
        assert_eq!(
            block_drops(ore, item("minecraft:stone_pickaxe"), &mut rng),
            [item("minecraft:raw_iron")]
        );
        assert!(block_drops(0, ItemStack::default(), &mut rng).is_empty());
    }

    #[test]
    fn cows_drop_leather_and_beef() {
        let leather = item("minecraft:leather");
        let beef = item("minecraft:beef");
        let mut rng = ferrite_entity::Rng::new(9);
        let mut saw_leather = false;
        for _ in 0..64 {
            let drops = mob_drops(ferrite_entity::mob::MobKind::Cow, &mut rng);
            assert!(!drops.is_empty(), "a cow always drops beef");
            assert!(
                drops
                    .iter()
                    .all(|stack| stack.item_id == beef.item_id || stack.item_id == leather.item_id),
                "unexpected cow drops: {drops:?}"
            );
            assert!(
                drops
                    .iter()
                    .any(|stack| stack.item_id == beef.item_id && stack.count >= 1)
            );
            saw_leather |= drops.iter().any(|stack| stack.item_id == leather.item_id);
        }
        assert!(saw_leather, "leather drops 2/3 of the time");
    }
}
