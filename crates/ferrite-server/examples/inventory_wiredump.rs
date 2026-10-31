//! Prints the clientbound container packets as `<phase> <name> <hex frame>` lines for
//! `python tools/wirecheck/run.py --frames`, which decodes them with the vanilla codecs.

use ferrite_item::{Component, ItemStack, Rarity, component_ids};
use ferrite_protocol::{Packet, VarInt, encode_packet, packets::play::*, types::Translatable};

fn line<P: Packet>(name: &str, p: &P) {
    let mut out = Vec::new();
    encode_packet(p, &mut out);
    let hex: String = out.iter().map(|b| format!("{b:02x}")).collect();
    println!("play {name} {hex}");
}

fn main() {
    let stone = ItemStack::from_name("stone").unwrap().copy_with_count(40);
    let mut sword = ItemStack::from_name("iron_sword").unwrap();
    sword
        .components
        .insert(component_ids::DAMAGE, Component::Damage(17));
    // Data-driven registries (enchantments) are missing in the offline checker, so the
    // components here come from static registries.
    sword
        .components
        .insert(component_ids::REPAIR_COST, Component::RepairCost(3));
    sword
        .components
        .insert(component_ids::RARITY, Component::Rarity(Rarity::Epic));
    let mut slots = vec![ItemStack::empty(); 46];
    slots[36] = stone.clone();
    slots[37] = sword.clone();
    line(
        "container_set_content",
        &ContainerContent {
            window: VarInt(0),
            state: VarInt(1),
            slots: slots.clone(),
            carried: ItemStack::empty(),
        },
    );
    line(
        "container_set_content",
        &ContainerContent {
            window: VarInt(3),
            state: VarInt(9),
            slots,
            carried: sword.clone(),
        },
    );
    for (slot, item) in [
        (36, stone.clone()),
        (0, ItemStack::empty()),
        (5, sword.clone()),
    ] {
        line(
            "container_set_slot",
            &ContainerSlot {
                window: VarInt(0),
                state: VarInt(2),
                slot,
                item,
            },
        );
    }
    line("set_cursor_item", &SetCursorItem { item: sword });
    line(
        "set_cursor_item",
        &SetCursorItem {
            item: ItemStack::empty(),
        },
    );
    line(
        "set_player_inventory",
        &SetPlayerInventory {
            slot: VarInt(40),
            item: stone,
        },
    );
    let crafting = ferrite_data::registries::id("minecraft:menu", "minecraft:crafting").unwrap();
    line(
        "open_screen",
        &OpenScreen {
            window: VarInt(1),
            menu: VarInt(crafting),
            title: Translatable("container.crafting".into()),
        },
    );
    line(
        "container_close",
        &ClientboundContainerClose { window: VarInt(1) },
    );
    line("set_held_slot", &SetHeldSlot { slot: VarInt(4) });
}
