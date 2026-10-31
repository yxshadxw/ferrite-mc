//! Stage-level throughput benchmark for the overworld generator.
//!
//! Usage: cargo run --release -p ferrite-worldgen --example genbench [seed] [count] [startX] [startZ]
//!
//! Prints per-stage milliseconds and the single-threaded chunks per second for the
//! pipeline: noise fill, surface rules, carvers, biome fill and feature decoration.

use ferrite_world::{BlockStateRegistry, Chunk, HeightmapKind, RuntimeRegistry};
use ferrite_worldgen::{FeatureGenerator, NoiseChunkGenerator, SurfaceGenerator, WorldgenData};
use std::path::PathBuf;
use std::time::Instant;

fn generated() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3/generated")
}

fn fill_biomes(
    chunk: &mut Chunk,
    surface: &SurfaceGenerator<'_>,
    registry: &RuntimeRegistry,
    cx: i32,
    cz: i32,
) {
    let min_y = chunk.min_y();
    for (section_index, section) in chunk.sections_mut().iter_mut().enumerate() {
        for qy in 0..4 {
            let y = min_y + section_index as i32 * 16 + qy * 4 + 2;
            for qz in 0..4 {
                let z = cz * 16 + qz * 4 + 2;
                for qx in 0..4 {
                    let x = cx * 16 + qx * 4 + 2;
                    let name = surface.biome_at(x, y, z);
                    let id = registry
                        .biome_id(&name.to_string())
                        .expect("biome in registry");
                    section
                        .set_biome((qy * 16 + qz * 4 + qx) as usize, id)
                        .unwrap();
                }
            }
        }
    }
}

#[derive(Default)]
struct Stage {
    millis: f64,
}

impl Stage {
    fn add(&mut self, start: Instant) -> f64 {
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        self.millis += elapsed;
        elapsed
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: i64 = args.get(1).map_or(0, |s| s.parse().unwrap());
    let count: usize = args.get(2).map_or(20, |s| s.parse().unwrap());
    let start_x: i32 = args.get(3).map_or(0, |s| s.parse().unwrap());
    let start_z: i32 = args.get(4).map_or(0, |s| s.parse().unwrap());

    let generated = generated();
    let data = WorldgenData::load(&generated).expect("worldgen data");
    let registry = RuntimeRegistry::load(&generated).expect("registry");
    let noise = NoiseChunkGenerator::new(&data, seed).expect("generator");

    // One-time setup cost for the reusable generators.
    let setup = Instant::now();
    let surface = SurfaceGenerator::new(&data, &noise, seed).expect("surface");
    println!(
        "surface setup: {:.2} ms",
        setup.elapsed().as_secs_f64() * 1000.0
    );
    let setup = Instant::now();
    let mut features = FeatureGenerator::new(&data, seed, -64, 384);
    println!(
        "feature setup: {:.2} ms",
        setup.elapsed().as_secs_f64() * 1000.0
    );

    let mut noise_fill = Stage::default();
    let mut surface_apply = Stage::default();
    let mut carve = Stage::default();
    let mut biomes = Stage::default();
    let mut decorate = Stage::default();
    let mut total = 0.0_f64;

    let pipeline_start = Instant::now();
    for index in 0..count {
        let cx = start_x + (index % 10) as i32;
        let cz = start_z + (index / 10) as i32;
        let chunk_start = Instant::now();

        let mark = Instant::now();
        let mut chunk = noise
            .fill_noise_chunk(cx, cz, &registry)
            .expect("noise fill");
        noise_fill.add(mark);

        let mark = Instant::now();
        surface
            .apply(&mut chunk, cx, cz, &registry)
            .expect("surface");
        surface_apply.add(mark);

        let mark = Instant::now();
        noise
            .carve_chunk(&mut chunk, cx, cz, &registry)
            .expect("carve");
        carve.add(mark);

        let mark = Instant::now();
        fill_biomes(&mut chunk, &surface, &registry, cx, cz);
        biomes.add(mark);

        let mark = Instant::now();
        features
            .decorate(
                &mut chunk,
                cx,
                cz,
                &|x, y, z| surface.biome_at(x, y, z),
                &registry,
            )
            .expect("decorate");
        decorate.add(mark);

        let _ = chunk.height(HeightmapKind::WorldSurface, 8, 8);
        total += chunk_start.elapsed().as_secs_f64() * 1000.0;
    }
    let wall = pipeline_start.elapsed().as_secs_f64();

    let n = count.max(1) as f64;
    println!("chunks: {count}");
    println!("noise fill:      {:8.2} ms/chunk", noise_fill.millis / n);
    println!("surface rules:   {:8.2} ms/chunk", surface_apply.millis / n);
    println!("carvers:         {:8.2} ms/chunk", carve.millis / n);
    println!("biome fill:      {:8.2} ms/chunk", biomes.millis / n);
    println!("feature decorate:{:8.2} ms/chunk", decorate.millis / n);
    println!("pipeline total:  {:8.2} ms/chunk", total / n);
    println!(
        "chunks/second:   {:8.2} (wall {:.2} s)",
        count as f64 / wall,
        wall
    );
}
