//! Integration tests against the real Minecraft 26.3 loot tables.

use ferrite_loot::{LootContext, LootTables, Rng};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

/// A SplitMix64 source, good enough to exercise the loot evaluator and stable
/// across machines.
struct TestRng(u64);

impl TestRng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }
}

impl Rng for TestRng {
    fn next_int(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "next_int called with bound {bound}");
        (self.next_u64() % u64::from(bound as u32)) as i32
    }

    fn next_float(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16_777_216.0
    }
}

fn tables() -> &'static LootTables {
    static TABLES: OnceLock<LootTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../vanilla/26.3/generated/data/minecraft/loot_table");
        assert!(
            dir.is_dir(),
            "vanilla data is missing at {}; run `python tools/fetch_vanilla.py`",
            dir.display()
        );
        LootTables::load(&dir).expect("load vanilla loot tables")
    })
}

fn table(id: &str) -> &'static ferrite_loot::LootTable {
    tables()
        .get(id)
        .unwrap_or_else(|| panic!("missing table {id}"))
}

#[test]
fn every_vanilla_table_parses() {
    let tables = tables();
    println!(
        "loaded {} loot tables from {} item tags and {} predicates",
        tables.len(),
        tables.tag_count(),
        tables.predicate_count()
    );

    assert!(tables.len() > 1_400, "got only {} tables", tables.len());
    assert!(tables.tag_count() > 100);
    assert_eq!(tables.predicate_count(), 3);
    for id in tables.ids() {
        assert!(id.starts_with("minecraft:"), "unnamespaced id {id}");
    }
    for id in [
        "minecraft:blocks/stone",
        "minecraft:blocks/diamond_ore",
        "minecraft:blocks/oak_leaves",
        "minecraft:chests/simple_dungeon",
    ] {
        assert!(tables.get(id).is_some(), "missing {id}");
    }
}

#[test]
fn stone_drops_cobblestone() {
    let table = table("minecraft:blocks/stone");
    assert_eq!(
        table.roll(&LootContext::default(), &mut TestRng::new(1)),
        [("minecraft:cobblestone".to_owned(), 1)]
    );

    let mut tool_enchantments = BTreeMap::new();
    tool_enchantments.insert("minecraft:silk_touch".to_owned(), 1);
    let silk_touch = LootContext {
        tool: Some("minecraft:diamond_pickaxe".to_owned()),
        tool_enchantments,
        ..Default::default()
    };
    assert_eq!(
        table.roll(&silk_touch, &mut TestRng::new(2)),
        [("minecraft:stone".to_owned(), 1)]
    );
}

#[test]
fn diamond_ore_with_fortune_three_yields_one_to_four() {
    let table = table("minecraft:blocks/diamond_ore");
    let context = LootContext {
        fortune: 3,
        ..Default::default()
    };
    let mut rng = TestRng::new(0xC0FF_EE00);
    let mut buckets = [0usize; 4];

    for _ in 0..1_000 {
        let drops = table.roll(&context, &mut rng);
        assert_eq!(drops.len(), 1, "unexpected drops {drops:?}");
        let (item, count) = &drops[0];
        assert_eq!(item, "minecraft:diamond");
        assert!((1..=4).contains(count), "count {count} out of range");
        buckets[*count as usize - 1] += 1;
    }

    println!("fortune III diamond counts: {buckets:?}");
    // ore_drops picks 1 twice as often as each higher multiplier.
    assert!(
        (350..=450).contains(&buckets[0]),
        "ones bucket {}",
        buckets[0]
    );
    for (index, bucket) in buckets.iter().enumerate().skip(1) {
        assert!(
            (150..=250).contains(bucket),
            "bucket {} is {bucket}",
            index + 1
        );
    }
    assert_eq!(buckets.iter().sum::<usize>(), 1_000);
}

#[test]
fn oak_leaves_sapling_chance_and_shears() {
    let table = table("minecraft:blocks/oak_leaves");
    let mut rng = TestRng::new(0xA0C_0DE);

    let mut saplings = 0;
    for _ in 0..1_000 {
        let drops = table.roll(&LootContext::default(), &mut rng);
        saplings += drops
            .iter()
            .filter(|(item, _)| item == "minecraft:oak_sapling")
            .map(|(_, count)| *count)
            .sum::<i32>();
    }
    println!("oak saplings without shears: {saplings}/1000");
    assert!((30..=70).contains(&saplings), "saplings: {saplings}");

    let shears = LootContext {
        tool: Some("minecraft:shears".to_owned()),
        ..Default::default()
    };
    for _ in 0..200 {
        assert_eq!(
            table.roll(&shears, &mut rng),
            [("minecraft:oak_leaves".to_owned(), 1)]
        );
    }
}

#[test]
fn chest_table_rolls_items_within_its_rolls_range() {
    let table = table("minecraft:chests/simple_dungeon");
    assert_eq!(table.pools().len(), 3);

    let mut rng = TestRng::new(0xD06E_0001);
    for _ in 0..200 {
        let drops = table.roll(&LootContext::default(), &mut rng);
        // Rolls are uniform 1..=3, 1..=4 and a constant 3, one stack each.
        assert!(
            (5..=10).contains(&drops.len()),
            "{} stacks: {drops:?}",
            drops.len()
        );
        for (item, count) in &drops {
            assert!(item.starts_with("minecraft:"), "bad id {item}");
            assert!((1..=8).contains(count), "{item} count {count}");
            match item.as_str() {
                "minecraft:leather" => assert!((1..=5).contains(count)),
                "minecraft:iron_ingot" | "minecraft:gold_ingot" => {
                    assert!((1..=4).contains(count));
                }
                "minecraft:melon_seeds"
                | "minecraft:pumpkin_seeds"
                | "minecraft:beetroot_seeds" => assert!((2..=4).contains(count)),
                _ => {}
            }
        }
    }
}
