use ferrite_world::{BlockStateRegistry, RuntimeRegistry};
#[test]
fn loads_vanilla_state_and_biome_registries() {
    let base =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3/generated");
    if !base.exists() {
        return;
    }
    let registry = RuntimeRegistry::load(base).unwrap();
    assert_eq!(
        registry.block_id("minecraft:air", &Default::default()),
        Some(0)
    );
    let bedrock = registry
        .block_id("minecraft:bedrock", &Default::default())
        .unwrap();
    assert_eq!(
        registry.block_state(bedrock).unwrap().name,
        "minecraft:bedrock"
    );
    assert!(registry.biome_id("minecraft:ocean").is_some());
}
#[test]
fn vanilla_chunk_conversion_roundtrip() {
    use ferrite_world::{BlockStateRegistry, RuntimeRegistry, chunk_from_nbt, chunk_to_nbt};
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3");
    let path = base.join("fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca");
    if !path.exists() {
        return;
    }
    let registry = RuntimeRegistry::load(base.join("generated")).unwrap();
    let bytes = ferrite_world::RegionFile::open(path, 0, 0)
        .unwrap()
        .read_chunk(0, 0)
        .unwrap()
        .unwrap();
    let (_, tag) = ferrite_nbt::binary::read_named(&bytes).unwrap();
    let chunk = chunk_from_nbt(&tag, -64, 384, &registry).unwrap();
    let bedrock = registry
        .block_id("minecraft:bedrock", &Default::default())
        .unwrap();
    for z in 0..16 {
        for x in 0..16 {
            assert_eq!(chunk.block(x, -64, z), Some(bedrock));
        }
    }
    let encoded = chunk_to_nbt(&chunk, &registry).unwrap();
    let restored = chunk_from_nbt(&encoded, -64, 384, &registry).unwrap();
    for y in -64..320 {
        for z in 0..16 {
            for x in 0..16 {
                assert_eq!(chunk.block(x, y, z), restored.block(x, y, z));
            }
        }
    }
}
#[test]
fn runtime_registry_heightmap_predicates() {
    use ferrite_world::{BlockInfo, BlockStateRegistry, RuntimeRegistry};
    let base =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3/generated");
    if !base.exists() {
        return;
    }
    let r = RuntimeRegistry::load(base).unwrap();
    let flags = |name| r.flags(r.block_id(name, &Default::default()).unwrap());
    assert!(flags("minecraft:stone").motion_blocking);
    assert!(!flags("minecraft:leaf_litter").motion_blocking);
    assert!(flags("minecraft:seagrass").fluid);
    assert!(flags("minecraft:oak_leaves").leaves);
}
