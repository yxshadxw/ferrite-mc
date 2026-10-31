use ferrite_nbt::{
    Tag,
    binary::{self, Compression},
};
use ferrite_world::{BlockInfo, BlockStateRegistry, RegionFile, RuntimeRegistry, chunk_from_nbt};
use ferrite_worldgen::{DensityFunction, NoiseChunkGenerator, WorldgenData};
use std::path::PathBuf;

fn base() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3")
}
fn fixture_seed() -> i64 {
    let bytes =
        std::fs::read(base().join("fixture-world/world/data/minecraft/world_gen_settings.dat"))
            .unwrap();
    let (_, tag) = binary::read_compressed(&bytes, Compression::Gzip).unwrap();
    let Tag::Compound(root) = tag else {
        panic!("worldgen settings root")
    };
    let Some(Tag::Compound(data)) = root.get("data") else {
        panic!("worldgen settings data")
    };
    let Some(Tag::Long(seed)) = data.get("seed") else {
        panic!("worldgen seed")
    };
    *seed
}
fn setup() -> (WorldgenData, RuntimeRegistry) {
    let generated = base().join("generated");
    (
        WorldgenData::load(&generated).unwrap(),
        RuntimeRegistry::load(generated).unwrap(),
    )
}

#[test]
fn constant_density_fills_stone_or_water_and_air() {
    let (mut data, registry) = setup();
    let key = "minecraft:overworld".parse().unwrap();
    let settings = data.noise_settings.get_mut(&key).unwrap();
    settings.noise.min_y = 0;
    settings.noise.height = 16;
    settings.sea_level = 8;
    settings.aquifers = None;
    settings.noise_router.final_density = DensityFunction::Constant(-1.0);
    let generated = NoiseChunkGenerator::new(&data, 1)
        .unwrap()
        .fill_noise_chunk(0, 0, &registry)
        .unwrap();
    let air = registry
        .block_id("minecraft:air", &Default::default())
        .unwrap();
    let water = registry
        .block_id("minecraft:water", &Default::default())
        .unwrap();
    assert_eq!(generated.block(0, 7, 0), Some(water));
    assert_eq!(generated.block(0, 8, 0), Some(air));
    data.noise_settings
        .get_mut(&key)
        .unwrap()
        .noise_router
        .final_density = DensityFunction::Constant(1.0);
    let solid = NoiseChunkGenerator::new(&data, 1)
        .unwrap()
        .fill_noise_chunk(0, 0, &registry)
        .unwrap();
    let stone = registry
        .block_id("minecraft:stone", &Default::default())
        .unwrap();
    assert_eq!(solid.block(0, 15, 0), Some(stone));
}

/// Compares the generated solid mask with the fixture world below y=40.
///
/// Structures (mineshafts, trial chambers, ...) modify saved terrain after the noise/aquifer
/// stage, so the 99% assertion applies only to chunks whose `structures.References` is empty.
/// The all-chunk rate is reported for comparison but deliberately not asserted.
#[test]
fn fixture_solid_mask_matches_vanilla_below_y_40() {
    let (data, registry) = setup();
    let generator = NoiseChunkGenerator::new(&data, fixture_seed()).unwrap();
    let region = RegionFile::open(
        base().join("fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca"),
        0,
        0,
    )
    .unwrap();
    let (mut matches, mut total, mut clear_matches, mut clear_total) = (0_u64, 0_u64, 0_u64, 0_u64);
    let mut names = std::collections::BTreeMap::<String, u64>::new();
    for (x, z) in [0, 1, 8, 15]
        .into_iter()
        .flat_map(|z| [0, 1, 8, 15].into_iter().map(move |x| (x, z)))
    {
        let bytes = region.read_chunk(x, z).unwrap().unwrap();
        let (_, tag) = binary::read_named(&bytes).unwrap();
        let Tag::Compound(root) = &tag else {
            panic!("chunk root")
        };
        let Some(Tag::Compound(structures)) = root.get("structures") else {
            panic!("structures")
        };
        let Some(Tag::Compound(references)) = structures.get("References") else {
            panic!("structure references")
        };
        let no_structure = references.is_empty();
        let expected = chunk_from_nbt(&tag, -64, 384, &registry).unwrap();
        let generated = generator.fill_chunk(x, z, &registry).unwrap();
        let before = matches;
        for yy in -64..40 {
            for zz in 0..16 {
                for xx in 0..16 {
                    let a = registry
                        .flags(generated.block(xx, yy, zz).unwrap())
                        .motion_blocking;
                    let b = registry
                        .flags(expected.block(xx, yy, zz).unwrap())
                        .motion_blocking;
                    if a != b {
                        *names
                            .entry(
                                registry
                                    .block_state(expected.block(xx, yy, zz).unwrap())
                                    .unwrap()
                                    .name
                                    .clone(),
                            )
                            .or_default() += 1;
                    }
                    matches += u64::from(a == b);
                    total += 1;
                    if no_structure {
                        clear_matches += u64::from(a == b);
                        clear_total += 1;
                    }
                }
            }
        }
        eprintln!(
            "chunk {x},{z}: {}/{}; structure refs: {}",
            matches - before,
            104 * 256,
            references.len()
        );
    }
    let rate = matches as f64 / total as f64;
    println!(
        "noise fill solid mask: {matches}/{total} = {:.5}%",
        rate * 100.0
    );
    println!(
        "noise fill solid mask without structure references: {clear_matches}/{clear_total} = {:.5}%",
        clear_matches as f64 * 100.0 / clear_total as f64
    );
    eprintln!("mismatch states: {names:?}");
    assert!(
        clear_total > 0,
        "fixture has no chunks without structure references"
    );
    let clear_rate = clear_matches as f64 / clear_total as f64;
    assert!(
        clear_rate >= 0.99,
        "structure-free solid mask below 99%: {clear_rate}"
    );
}
