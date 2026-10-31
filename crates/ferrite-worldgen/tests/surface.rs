use ferrite_world::{BlockStateRegistry, RuntimeRegistry};
use ferrite_worldgen::{
    DensityFunction, MaterialRule, MaterialRuleNode, NoiseChunkGenerator, ResourceId,
    SurfaceGenerator, WorldgenData,
};
use std::path::PathBuf;

fn generated() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3/generated")
}

#[test]
fn block_rule_changes_filled_stone() {
    let path = generated();
    let mut data = WorldgenData::load(&path).unwrap();
    let registry = RuntimeRegistry::load(&path).unwrap();
    let settings = data
        .noise_settings
        .get_mut(&ResourceId::parse("overworld").unwrap())
        .unwrap();
    settings.noise.min_y = 0;
    settings.noise.height = 16;
    settings.aquifers = None;
    settings.noise_router.final_density = DensityFunction::Constant(1.0);
    let rule = settings.material_rule.clone();
    data.material_rules.insert(
        rule,
        MaterialRule::Node(MaterialRuleNode::Block {
            result_state: ResourceId::parse("dirt").unwrap(),
        }),
    );
    let noise = NoiseChunkGenerator::new(&data, 42).unwrap();
    let surface = SurfaceGenerator::new(&data, &noise, 42).unwrap();
    let mut chunk = noise.fill_noise_chunk(0, 0, &registry).unwrap();
    surface.apply(&mut chunk, 0, 0, &registry).unwrap();
    let dirt = registry
        .block_id("minecraft:dirt", &Default::default())
        .unwrap();
    assert_eq!(chunk.block(0, 15, 0), Some(dirt));
    assert_eq!(chunk.block(15, 0, 15), Some(dirt));
}

#[test]
fn y_above_rule_only_changes_stone_at_or_above_anchor() {
    use ferrite_worldgen::{MaterialCondition, MaterialConditionNode, VerticalAnchor};
    let path = generated();
    let mut data = WorldgenData::load(&path).unwrap();
    let registry = RuntimeRegistry::load(&path).unwrap();
    let settings = data
        .noise_settings
        .get_mut(&ResourceId::parse("overworld").unwrap())
        .unwrap();
    settings.noise.min_y = 0;
    settings.noise.height = 16;
    settings.aquifers = None;
    settings.noise_router.final_density = DensityFunction::Constant(1.0);
    let root = settings.material_rule.clone();
    data.material_rules.insert(
        root,
        MaterialRule::Node(MaterialRuleNode::Condition {
            if_true: MaterialCondition::Node(MaterialConditionNode::YAbove {
                anchor: VerticalAnchor::Absolute { absolute: 8 },
                surface_depth_multiplier: 0,
                add_stone_depth: false,
            }),
            then_run: Box::new(MaterialRule::Node(MaterialRuleNode::Block {
                result_state: ResourceId::parse("dirt").unwrap(),
            })),
        }),
    );
    let noise = NoiseChunkGenerator::new(&data, 42).unwrap();
    let surface = SurfaceGenerator::new(&data, &noise, 42).unwrap();
    let mut chunk = noise.fill_noise_chunk(0, 0, &registry).unwrap();
    surface.apply(&mut chunk, 0, 0, &registry).unwrap();
    let dirt = registry
        .block_id("minecraft:dirt", &Default::default())
        .unwrap();
    let stone = registry
        .block_id("minecraft:stone", &Default::default())
        .unwrap();
    assert_eq!(chunk.block(0, 8, 0), Some(dirt));
    assert_eq!(chunk.block(0, 7, 0), Some(stone));
}

#[test]
fn floor_stone_depth_changes_only_the_top_layer() {
    use ferrite_worldgen::{MaterialCondition, MaterialConditionNode, SurfaceType};
    let path = generated();
    let mut data = WorldgenData::load(&path).unwrap();
    let registry = RuntimeRegistry::load(&path).unwrap();
    let settings = data
        .noise_settings
        .get_mut(&ResourceId::parse("overworld").unwrap())
        .unwrap();
    settings.noise.min_y = 0;
    settings.noise.height = 16;
    settings.aquifers = None;
    settings.noise_router.final_density = DensityFunction::Constant(1.0);
    let root = settings.material_rule.clone();
    data.material_rules.insert(
        root,
        MaterialRule::Node(MaterialRuleNode::Condition {
            if_true: MaterialCondition::Node(MaterialConditionNode::StoneDepth {
                offset: 0,
                surface_type: SurfaceType::Floor,
                secondary_depth_range: 0,
                add_surface_depth: false,
            }),
            then_run: Box::new(MaterialRule::Node(MaterialRuleNode::Block {
                result_state: ResourceId::parse("dirt").unwrap(),
            })),
        }),
    );
    let noise = NoiseChunkGenerator::new(&data, 42).unwrap();
    let surface = SurfaceGenerator::new(&data, &noise, 42).unwrap();
    let mut chunk = noise.fill_noise_chunk(0, 0, &registry).unwrap();
    surface.apply(&mut chunk, 0, 0, &registry).unwrap();
    let dirt = registry
        .block_id("minecraft:dirt", &Default::default())
        .unwrap();
    let stone = registry
        .block_id("minecraft:stone", &Default::default())
        .unwrap();
    assert_eq!(chunk.block(0, 15, 0), Some(dirt));
    assert_eq!(chunk.block(0, 14, 0), Some(stone));
}

#[test]
fn fixture_top_blocks_match_vanilla_surface() {
    use ferrite_nbt::{
        Tag,
        binary::{self, Compression},
    };
    use ferrite_world::{BlockInfo, RegionFile, chunk_from_nbt};
    let base = generated().parent().unwrap().to_owned();
    let seed_path = base.join("fixture-world/world/data/minecraft/world_gen_settings.dat");
    let (_, tag) =
        binary::read_compressed(&std::fs::read(seed_path).unwrap(), Compression::Gzip).unwrap();
    let Tag::Compound(root) = tag else {
        panic!("worldgen root")
    };
    let Some(Tag::Compound(settings)) = root.get("data") else {
        panic!("worldgen data")
    };
    let Some(Tag::Long(seed)) = settings.get("seed") else {
        panic!("seed")
    };
    let data = WorldgenData::load(generated()).unwrap();
    let registry = RuntimeRegistry::load(generated()).unwrap();
    let noise = NoiseChunkGenerator::new(&data, *seed).unwrap();
    let surface = SurfaceGenerator::new(&data, &noise, *seed).unwrap();
    let region = RegionFile::open(
        base.join("fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca"),
        0,
        0,
    )
    .unwrap();
    let mut total = 0;
    let mut matched = 0;
    let mut clear_total = 0;
    let mut clear_matched = 0;
    let mut terrain_total = 0;
    let mut terrain_matched = 0;
    let mut mismatches =
        std::collections::BTreeMap::<(String, String, String, String), usize>::new();
    for (cx, cz) in [0, 1, 8, 15]
        .into_iter()
        .flat_map(|z| [0, 1, 8, 15].into_iter().map(move |x| (x, z)))
    {
        let bytes = region.read_chunk(cx, cz).unwrap().unwrap();
        let (_, tag) = binary::read_named(&bytes).unwrap();
        let Tag::Compound(root) = &tag else {
            panic!("chunk")
        };
        let Some(Tag::Compound(structures)) = root.get("structures") else {
            panic!("structures")
        };
        let Some(Tag::Compound(refs)) = structures.get("References") else {
            panic!("references")
        };
        let clear = refs.is_empty();
        let expected = chunk_from_nbt(&tag, -64, 384, &registry).unwrap();
        let mut actual = noise.fill_noise_chunk(cx, cz, &registry).unwrap();
        surface.apply(&mut actual, cx, cz, &registry).unwrap();
        noise.carve_chunk(&mut actual, cx, cz, &registry).unwrap();
        let top = |chunk: &ferrite_world::Chunk, x: u8, z: u8| -> (String, i32) {
            for y in (-64..320).rev() {
                let id = chunk.block(x, y, z).unwrap();
                let flags = registry.flags(id);
                if !flags.motion_blocking || flags.leaves || flags.fluid {
                    continue;
                }
                let name = &registry.block_state(id).unwrap().name;
                if name.ends_with("_log")
                    || name.ends_with("_wood")
                    || name.ends_with("_stem")
                    || name.ends_with("_hyphae")
                    || name.contains("mushroom")
                    || name.contains("azalea")
                    || name == "minecraft:cactus"
                    || name == "minecraft:bamboo"
                {
                    continue;
                }
                return (name.clone(), y);
            }
            ("minecraft:air".to_owned(), -64)
        };
        let before = matched;
        for z in 0..16 {
            for x in 0..16 {
                let (a, _) = top(&actual, x, z);
                let (b, y) = top(&expected, x, z);
                if a != b {
                    let sec = ((y + 64) / 16) as usize;
                    let index = (((y + 64) % 16) / 4 * 16 + i32::from(z / 4) * 4 + i32::from(x / 4))
                        as usize;
                    let id = expected.sections()[sec].biomes().get(index).unwrap();
                    let old_biome = registry.biome_name(id).unwrap_or("unknown").to_owned();
                    let new_biome = surface
                        .biome_at(cx * 16 + i32::from(x), y, cz * 16 + i32::from(z))
                        .to_string();
                    *mismatches
                        .entry((a.clone(), b.clone(), new_biome, old_biome))
                        .or_default() += 1;
                }
                matched += usize::from(a == b);
                total += 1;
                if clear {
                    clear_matched += usize::from(a == b);
                    clear_total += 1;
                    // Frozen-ocean ice/blue-ice features and recognizable structure
                    // materials can cross into a chunk without a structure reference.
                    // Determine exclusions solely from the fixture, before comparing.
                    let feature = matches!(
                        b.as_str(),
                        "minecraft:ice"
                            | "minecraft:blue_ice"
                            | "minecraft:netherrack"
                            | "minecraft:magma_block"
                            | "minecraft:obsidian"
                            | "minecraft:crying_obsidian"
                            | "minecraft:stone_bricks"
                            | "minecraft:mossy_stone_bricks"
                            | "minecraft:chiseled_stone_bricks"
                            | "minecraft:mossy_cobblestone"
                            | "minecraft:pumpkin"
                    );
                    if !feature {
                        terrain_total += 1;
                        terrain_matched += usize::from(a == b);
                    }
                }
            }
        }
        eprintln!(
            "surface chunk {cx},{cz}: {}/256; structure refs {}",
            matched - before,
            refs.len()
        );
    }
    let rate = matched as f64 / total as f64;
    let clear_rate = clear_matched as f64 / clear_total as f64;
    println!(
        "surface top blocks: all {matched}/{total} = {:.5}%; no structure references {clear_matched}/{clear_total} = {:.5}%",
        rate * 100.0,
        clear_rate * 100.0
    );
    eprintln!("surface mismatches: {mismatches:?}");
    let terrain_rate = terrain_matched as f64 / terrain_total as f64;
    println!(
        "surface top blocks excluding referenced-structure chunks and recognizable feature blocks: {terrain_matched}/{terrain_total} = {:.5}%",
        terrain_rate * 100.0
    );
    assert!(
        terrain_total > 0 && terrain_rate >= 0.98,
        "surface match below 98%: {terrain_rate}"
    );
}
