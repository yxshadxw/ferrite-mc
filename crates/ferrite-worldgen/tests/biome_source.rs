//! Multi-noise biome source behavior against the vanilla generated data.

use std::path::{Path, PathBuf};

use ferrite_worldgen::{ClimateTargetPoint, MultiNoiseBiomeSource, WorldgenData};

fn generated_root() -> PathBuf {
    if let Some(root) = std::env::var_os("FERRITE_VANILLA_ROOT") {
        return PathBuf::from(root);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3/generated")
}

fn worldgen() -> WorldgenData {
    WorldgenData::load(generated_root()).expect("load vanilla worldgen data")
}

#[test]
fn loads_overworld_and_nether_presets() {
    let data = worldgen();
    let overworld = MultiNoiseBiomeSource::from_worldgen(&data, "minecraft:overworld")
        .expect("overworld preset");
    let nether =
        MultiNoiseBiomeSource::from_worldgen(&data, "minecraft:nether").expect("nether preset");

    assert_eq!(overworld.len(), 7_594);
    assert_eq!(nether.len(), 5);
    assert!(!overworld.is_empty());
    assert!(overworld.biomes().any(|biome| biome.path() == "plains"));
    assert!(nether.biomes().any(|biome| biome.path() == "nether_wastes"));
    assert!(MultiNoiseBiomeSource::from_worldgen(&data, "minecraft:missing").is_err());
}

#[test]
fn nether_search_matches_brute_force() {
    let data = worldgen();
    let source =
        MultiNoiseBiomeSource::from_worldgen(&data, "minecraft:nether").expect("nether preset");
    let mut random = SplitMix64::new(0x5EED_2026_1029);
    for sample in 0..1_000 {
        let target = ClimateTargetPoint::quantized(
            random.range(0.0, 1.0),
            random.range(0.0, 1.0),
            random.range(0.0, 1.0),
            random.range(0.0, 1.0),
            random.range(0.0, 1.0),
            random.range(0.0, 1.0),
        );
        assert_eq!(
            source.find_biome(target),
            source.find_biome_brute_force(target),
            "nether sample {sample} disagreed for {target:?}"
        );
    }
}

#[test]
fn accelerated_search_matches_brute_force_on_100k_random_points() {
    let data = worldgen();
    let source = MultiNoiseBiomeSource::from_worldgen(&data, "minecraft:overworld")
        .expect("overworld preset");
    let mut random = SplitMix64::new(0x5EED_2026_1028);

    for sample in 0..100_000 {
        let target = ClimateTargetPoint::quantized(
            random.range(-2.0, 2.0),
            random.range(-2.0, 2.0),
            random.range(-2.0, 2.0),
            random.range(-2.0, 2.0),
            random.range(0.0, 1.0),
            random.range(-2.0, 2.0),
        );
        let accelerated = source.find_biome(target);
        let brute_force = source.find_biome_brute_force(target);
        assert_eq!(
            accelerated, brute_force,
            "sample {sample} disagreed for {target:?}"
        );
    }
    println!("100000/100000 accelerated and brute-force samples matched");
}

struct SplitMix64(u64);

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn unit(&mut self) -> f32 {
        ((self.next_u64() >> 40) as u32) as f32 / 16_777_216.0
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.unit()
    }
}
