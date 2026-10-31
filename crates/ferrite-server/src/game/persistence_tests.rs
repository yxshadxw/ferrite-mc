//! End-to-end persistence tests.
//!
//! Each test writes a world into a fresh temporary directory, drops the chunk source and opens
//! the same directory again, the way a server restart does. Nothing here shares process state:
//! every world gets its own directory and no player data directory.

use super::anvil::AnvilChunkSource;
use super::{blocks, entity_save, furnace};
use ferrite_entity::{
    Rng, item,
    mob::{self, MobKind},
};
use ferrite_inventory::{FurnaceKind, FurnaceState};
use ferrite_math::Vec3;
use ferrite_nbt::{Compound, List, Tag};
use ferrite_region::ChunkPos;
use std::path::{Path, PathBuf};

fn vanilla_root() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3/generated");
    root.join("reports/blocks.json").exists().then_some(root)
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ferrite-persistence-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn await_chunk(source: &std::sync::Arc<AnvilChunkSource>, x: i32, z: i32) {
    loop {
        if let Some(result) = source.get(x, z, 0) {
            result.expect("chunk load");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
}

fn state(name: &str) -> u32 {
    u32::from(
        ferrite_data::blocks::default_state(name)
            .unwrap_or_else(|| panic!("vanilla has a {name} block state")),
    )
}

fn sign_text(lines: &[&str]) -> Compound {
    let mut text = Compound::new();
    text.insert(
        "messages".into(),
        Tag::List(List::String(
            lines.iter().map(|line| (*line).to_owned()).collect(),
        )),
    );
    text.insert("color".into(), Tag::String("black".into()));
    text.insert("has_glowing_text".into(), Tag::Byte(0));
    text
}

fn write_sign(source: &AnvilChunkSource, [x, y, z]: [i32; 3], front: &[&str], back: &[&str]) {
    let type_id = ferrite_data::registries::id("minecraft:block_entity_type", "minecraft:sign")
        .and_then(|id| u32::try_from(id).ok())
        .expect("sign block entity type");
    source
        .update_block_entity(x, y, z, |entity| {
            let entity = entity.get_or_insert_with(|| {
                let mut data = Compound::new();
                data.insert("id".into(), Tag::String("minecraft:sign".into()));
                data.insert("x".into(), Tag::Int(x));
                data.insert("y".into(), Tag::Int(y));
                data.insert("z".into(), Tag::Int(z));
                ferrite_world::BlockEntity { type_id, data }
            });
            entity
                .data
                .insert("front_text".into(), Tag::Compound(sign_text(front)));
            entity
                .data
                .insert("back_text".into(), Tag::Compound(sign_text(back)));
            (true, ())
        })
        .unwrap();
}

fn store_hopper_items(source: &AnvilChunkSource, [x, y, z]: [i32; 3]) {
    let type_id = ferrite_data::registries::id("minecraft:block_entity_type", "minecraft:hopper")
        .and_then(|id| u32::try_from(id).ok())
        .expect("hopper block entity type");
    let mut entry = ferrite_item::ItemStack::from_name("iron_ingot")
        .unwrap()
        .copy_with_count(7)
        .to_nbt()
        .unwrap();
    entry.insert("Slot".into(), Tag::Byte(0));
    source
        .update_block_entity(x, y, z, |entity| {
            let entity = entity.get_or_insert_with(|| {
                let mut data = Compound::new();
                data.insert("id".into(), Tag::String("minecraft:hopper".into()));
                data.insert("x".into(), Tag::Int(x));
                data.insert("y".into(), Tag::Int(y));
                data.insert("z".into(), Tag::Int(z));
                ferrite_world::BlockEntity { type_id, data }
            });
            entity.data.insert(
                "Items".into(),
                Tag::List(List::Compound(vec![entry.clone()])),
            );
            (true, ())
        })
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entities_and_block_entities_survive_a_world_reload() {
    let Some(root) = vanilla_root() else {
        eprintln!("SKIP persistence test: vanilla/26.3/generated is absent");
        return;
    };
    let world = temp_dir("reload");
    let registry = ferrite_world::RuntimeRegistry::load(&root).unwrap();
    let source = AnvilChunkSource::new(world.clone(), registry);
    await_chunk(&source, 0, 0).await;

    // A furnace caught mid-smelt.
    source.set_block(2, 64, 2, state("furnace")).unwrap();
    let furnace_data = furnace::FurnaceData {
        items: vec![
            ferrite_item::ItemStack::from_name("raw_gold").unwrap(),
            ferrite_item::ItemStack::from_name("coal")
                .unwrap()
                .copy_with_count(3),
            ferrite_item::ItemStack::empty(),
        ],
        state: FurnaceState {
            cook: 117,
            cook_total: 200,
            burn: 400,
            burn_total: 1600,
            pending_experience: 1.5,
        },
    };
    furnace::store(&source, [2, 64, 2], FurnaceKind::Furnace, &furnace_data);

    // A sign with both sides filled in.
    source.set_block(3, 64, 3, state("oak_sign")).unwrap();
    write_sign(&source, [3, 64, 3], &["hello", "", "ferrite"], &["back"]);

    // A hopper with a stack in slot 0.
    source.set_block(4, 64, 4, state("hopper")).unwrap();
    store_hopper_items(&source, [4, 64, 4]);

    // A cow and a dropped item, drained exactly as an unload does.
    let mut rng = Rng::default();
    let cow = mob::new_mob(MobKind::Cow, Vec3::new(8.5, 64.0, 8.5), &mut rng);
    let cow_uuid = cow.data.uuid;
    let stone = ferrite_item::ItemStack::from_name("stone")
        .unwrap()
        .copy_with_count(3);
    let dropped = item::pop_resource(
        [8, 64, 9],
        ferrite_protocol::packets::play::ItemStack {
            count: stone.count,
            item_id: i32::from(stone.item),
        },
        &mut rng,
    );
    let drop_uuid = dropped.data.uuid;
    let drop_pos = dropped.pos;
    let mut region = super::region::Region::default();
    region.entities.insert(cow);
    region.entities.insert(dropped);
    let entities = region.drain_entities();
    assert_eq!(entities.len(), 2);
    let saved = region.take_chunk_entities(ChunkPos(0, 0));
    assert!(saved.is_empty(), "drained entities must not appear again");
    entity_save::save(&world, (0, 0), &entities).unwrap();

    source.flush_chunk(0, 0).await.unwrap();
    drop(source);

    // "Restart": a new source over the same directory.
    let registry = ferrite_world::RuntimeRegistry::load(&root).unwrap();
    let source = AnvilChunkSource::new(world.clone(), registry);
    await_chunk(&source, 0, 0).await;

    assert_eq!(
        furnace::load(&source, [2, 64, 2]),
        furnace_data,
        "furnace items and timers"
    );
    let sign = source.block_entity(3, 64, 3).expect("sign block entity");
    assert_eq!(
        blocks::sign_messages(&sign, true),
        Some(vec![
            "hello".to_string(),
            String::new(),
            "ferrite".to_string()
        ])
    );
    assert_eq!(
        blocks::sign_messages(&sign, false),
        Some(vec!["back".to_string()])
    );

    let hopper = source.block_entity(4, 64, 4).expect("hopper block entity");
    let Tag::List(List::Compound(items)) = hopper.data.get("Items").unwrap() else {
        panic!("hopper Items list");
    };
    let stack = ferrite_item::ItemStack::from_nbt(&items[0]).unwrap();
    assert_eq!(stack.name(), Some("minecraft:iron_ingot"));
    assert_eq!(stack.count, 7);

    let entities = entity_save::load(&world, (0, 0));
    assert_eq!(entities.len(), 2);
    let cow = entities
        .iter()
        .find(|record| record.data.uuid == cow_uuid)
        .expect("saved cow");
    assert_eq!(cow.data.mob.as_ref().unwrap().kind, MobKind::Cow);
    assert_eq!(
        cow.data.mob.as_ref().unwrap().health,
        MobKind::Cow.max_health()
    );
    assert_eq!(cow.pos, Vec3::new(8.5, 64.0, 8.5));
    let dropped = entities
        .iter()
        .find(|record| record.data.uuid == drop_uuid)
        .expect("saved item");
    assert_eq!(dropped.pos, drop_pos);
    assert_eq!(ferrite_entity::item::stack(&dropped.data).count, 3);

    // The furnace really resumes ticking: the restored state drives a menu tick.
    let mut menu = ferrite_inventory::Menu::furnace(1, FurnaceKind::Furnace);
    let loaded = furnace::load(&source, [2, 64, 2]);
    menu.container_mut()[..3].clone_from_slice(&loaded.items);
    menu.set_furnace_state(loaded.state.clone());
    menu.tick(super::container::crafter());
    assert_eq!(
        menu.furnace_state().unwrap().burn,
        loaded.state.burn - 1,
        "a restored furnace keeps burning"
    );

    drop(source);
    let _ = std::fs::remove_dir_all(world);
}

#[test]
fn unloading_and_reloading_a_chunk_twice_keeps_one_entity() {
    let world = temp_dir("twice");
    let mut rng = Rng::default();
    let mut region = super::region::Region::default();
    let record = mob::new_mob(MobKind::Zombie, Vec3::new(1.5, 64.0, 1.5), &mut rng);
    let uuid = record.data.uuid;
    region.entities.insert(record);

    for cycle in 0..2 {
        let saved = region.take_chunk_entities(ChunkPos(0, 0));
        assert_eq!(saved.len(), 1, "cycle {cycle} saves the entity once");
        assert!(region.entities.is_empty());
        entity_save::save(&world, (0, 0), &saved).unwrap();
        let loaded = entity_save::load(&world, (0, 0));
        assert_eq!(loaded.len(), 1);
        region.add_entities(loaded);
        assert_eq!(region.entities.len(), 1, "cycle {cycle} spawns it once");
    }
    assert_eq!(
        region
            .entities
            .data(region.entities.keys()[0])
            .unwrap()
            .uuid,
        uuid
    );
    // Only the chunk that was released loses its entities.
    let elsewhere = mob::new_mob(MobKind::Pig, Vec3::new(40.5, 64.0, 1.5), &mut rng);
    region.entities.insert(elsewhere);
    assert!(region.take_chunk_entities(ChunkPos(5, 5)).is_empty());
    assert_eq!(region.entities.len(), 2);
    assert_eq!(region.take_chunk_entities(ChunkPos(2, 0)).len(), 1);
    assert_eq!(region.entities.len(), 1);

    let _ = std::fs::remove_dir_all(world);
}
