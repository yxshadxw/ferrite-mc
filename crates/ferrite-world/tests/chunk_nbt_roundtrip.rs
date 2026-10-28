use ferrite_nbt::{Compound, List, Tag, binary};
use ferrite_world::{
    BlockEntity, BlockStateRegistry, RegionFile, RuntimeRegistry, chunk_from_nbt, chunk_to_nbt,
};

fn fixture(full: bool) -> (Tag, RuntimeRegistry) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3");
    let registry = RuntimeRegistry::load(base.join("generated")).unwrap();
    let region = RegionFile::open(
        base.join("fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca"),
        0,
        0,
    )
    .unwrap();
    for z in 0..32 {
        for x in 0..32 {
            if let Some(bytes) = region.read_chunk(x, z).unwrap() {
                let (_, tag) = binary::read_named(&bytes).unwrap();
                if let Tag::Compound(root) = &tag
                    && (root.get("Status") == Some(&Tag::String("minecraft:full".into()))) == full
                {
                    return (tag, registry);
                }
            }
        }
    }
    panic!("fixture lacks requested chunk status");
}

#[test]
fn full_chunk_preserves_entities_biomes_light_and_unknown_metadata() {
    let (mut input, registry) = fixture(true);
    let Tag::Compound(root) = &mut input else {
        unreachable!()
    };
    root.insert(
        "ferrite_test_unknown".into(),
        Tag::String("preserved".into()),
    );
    let Tag::List(List::Compound(sections)) = root.get_mut("sections").unwrap() else {
        panic!()
    };
    sections[0].insert("ferrite_test_section".into(), Tag::Int(73));
    sections[0].insert("SkyLight".into(), Tag::ByteArray(vec![17; 2048]));
    sections[0].insert("BlockLight".into(), Tag::ByteArray(vec![34; 2048]));
    let originals = sections.clone();
    let mut chunk = chunk_from_nbt(&input, -64, 384, &registry).unwrap();
    let water = ferrite_data::blocks::default_state("water").unwrap() as u32;
    let slab = ferrite_data::blocks::state_id(
        "stone_slab",
        &[("type", "bottom"), ("waterlogged", "true")],
    )
    .unwrap() as u32;
    chunk.set_block(0, 319, 0, water, &registry).unwrap();
    chunk.set_block(1, 319, 0, slab, &registry).unwrap();
    assert_eq!(chunk.sections().last().unwrap().fluid_count(), 2);
    let mut data = Compound::new();
    data.insert("CustomName".into(), Tag::String("example".into()));
    chunk.block_entities.insert(
        (1, 64, 2),
        BlockEntity {
            type_id: registry.entity_id("minecraft:chest").unwrap(),
            data,
        },
    );
    let encoded = chunk_to_nbt(&chunk, &registry).unwrap();
    let Tag::Compound(root) = &encoded else {
        panic!()
    };
    assert_eq!(
        root.get("ferrite_test_unknown"),
        Some(&Tag::String("preserved".into()))
    );
    let Tag::List(List::Compound(sections)) = root.get("sections").unwrap() else {
        panic!()
    };
    for old in originals {
        let new = sections
            .iter()
            .find(|s| s.get("Y") == old.get("Y"))
            .unwrap();
        for key in ["SkyLight", "BlockLight", "ferrite_test_section"] {
            assert_eq!(new.get(key), old.get(key));
        }
    }
    let restored = chunk_from_nbt(&encoded, -64, 384, &registry).unwrap();
    assert_eq!(restored.sections().last().unwrap().fluid_count(), 2);
    assert_eq!(
        restored.block_entities[&(1, 64, 2)].data.get("CustomName"),
        Some(&Tag::String("example".into()))
    );
    for (a, b) in chunk.sections().iter().zip(restored.sections()) {
        for i in 0..64 {
            assert_eq!(a.biomes().get(i), b.biomes().get(i));
        }
        assert_eq!(a.fluid_count(), b.fluid_count());
    }
}

#[test]
fn proto_chunk_from_fixture_roundtrips_status_and_blocks() {
    let (input, registry) = fixture(false);
    let Tag::Compound(root) = &input else {
        panic!()
    };
    let status = root.get("Status").cloned();
    let chunk = chunk_from_nbt(&input, -64, 384, &registry).unwrap();
    let encoded = chunk_to_nbt(&chunk, &registry).unwrap();
    let Tag::Compound(root) = &encoded else {
        panic!()
    };
    assert_eq!(root.get("Status"), status.as_ref());
    let restored = chunk_from_nbt(&encoded, -64, 384, &registry).unwrap();
    for (a, b) in chunk.sections().iter().zip(restored.sections()) {
        for i in 0..4096 {
            assert_eq!(a.blocks().get(i), b.blocks().get(i));
        }
        for i in 0..64 {
            assert_eq!(a.biomes().get(i), b.biomes().get(i));
        }
    }
}
