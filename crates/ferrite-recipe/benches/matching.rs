//! Measures indexed crafting lookup over 10,000 random 3x3 grids.

use ferrite_recipe::RecipeBook;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

const ITEMS: &[&str] = &[
    "minecraft:oak_planks",
    "minecraft:spruce_planks",
    "minecraft:acacia_planks",
    "minecraft:stick",
    "minecraft:cobblestone",
    "minecraft:stone",
    "minecraft:iron_ingot",
    "minecraft:gold_ingot",
    "minecraft:copper_ingot",
    "minecraft:diamond",
    "minecraft:redstone",
    "minecraft:coal",
    "minecraft:gunpowder",
    "minecraft:flint",
    "minecraft:string",
    "minecraft:paper",
    "minecraft:iron_block",
    "minecraft:gold_block",
    "minecraft:oak_log",
    "minecraft:acacia_log",
    "minecraft:sand",
    "minecraft:glass",
    "minecraft:bucket",
    "minecraft:iron_axe",
    "minecraft:iron_pickaxe",
    "minecraft:raw_iron",
    "minecraft:raw_copper",
    "minecraft:raw_gold",
    "minecraft:leather",
    "minecraft:wheat",
    "minecraft:dirt",
    "minecraft:bedrock",
];

fn main() {
    let data =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3/generated/data/minecraft");
    let book = RecipeBook::load(&data).expect("load vanilla recipes");
    let item_count = u64::try_from(ITEMS.len()).expect("item count fits in u64");
    let mut rng = 0x9e37_79b9_7f4a_7c15u64;

    let start = Instant::now();
    let mut matched = 0usize;
    for _ in 0..10_000 {
        let mut grid = [None; 9];
        for cell in &mut grid {
            // Fill 70% of the slots, like a plausible player inventory.
            if next_random(&mut rng) % 10 < 7 {
                let index =
                    usize::try_from(next_random(&mut rng) % item_count).expect("item index fits");
                *cell = Some(ITEMS[index]);
            }
        }
        if book.match_crafting_3x3(black_box(&grid)).is_some() {
            matched += 1;
        }
    }
    let elapsed = start.elapsed();
    let total_ms = elapsed.as_secs_f64() * 1_000.0;
    let micros = elapsed.as_secs_f64() * 1_000_000.0 / 10_000.0;

    println!(
        "10,000 random 3x3 grids: {total_ms:.2} ms total, {micros:.3} us/grid, {matched} matched"
    );
}

fn next_random(state: &mut u64) -> u64 {
    let mut value = *state;
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    *state = value;
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
