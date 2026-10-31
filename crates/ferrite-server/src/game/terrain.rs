//! Vanilla overworld terrain for newly generated chunks.
//!
//! The pipeline mirrors `NoiseBasedChunkGenerator.buildTerrain` in the 26.3
//! server: a `NoiseChunk` fill (`NoiseChunkGenerator::fill_noise_chunk`), then
//! `MaterialSystem.buildSurface` ([`ferrite_worldgen::SurfaceGenerator`]), then
//! the carvers. It runs on a dedicated thread so a region tick never evaluates
//! density functions; callers wait for a result from a `spawn_blocking` task.
use ferrite_world::{
    BlockInfo, BlockStateRegistry, Chunk, HeightmapKind, LightData, LightSection, RuntimeRegistry,
};
use ferrite_worldgen::{FeatureGenerator, NoiseChunkGenerator, SurfaceGenerator, WorldgenData};
use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, Sender, channel},
};

/// How new chunks are produced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerrainKind {
    /// The in-memory test world used before world generation existed.
    #[default]
    Superflat,
    /// The 26.3 overworld noise generator.
    Vanilla,
}

/// A generated column plus the light data the wire encoder needs.
pub struct Generated {
    pub chunk: Chunk,
    pub light: LightData,
}

enum Job {
    Chunk {
        x: i32,
        z: i32,
        reply: Sender<Result<Generated, String>>,
    },
    Height {
        x: i32,
        z: i32,
        reply: Sender<Result<i32, String>>,
    },
}

/// A handle to the dedicated vanilla terrain thread.
pub struct VanillaTerrain {
    jobs: Sender<Job>,
}

fn vanilla_root() -> PathBuf {
    std::env::var_os("FERRITE_VANILLA_ROOT").map_or_else(
        || std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3/generated"),
        PathBuf::from,
    )
}

impl VanillaTerrain {
    /// Loads the vanilla data and starts the generator thread.
    pub fn spawn(seed: i64) -> anyhow::Result<Self> {
        let root = vanilla_root();
        let data = WorldgenData::load(&root)
            .map_err(|error| anyhow::anyhow!("loading worldgen data: {error}"))?;
        let registry = RuntimeRegistry::load(&root)
            .map_err(|error| anyhow::anyhow!("loading vanilla registry: {error}"))?;
        let (jobs, receiver) = channel();
        std::thread::Builder::new()
            .name("ferrite-worldgen".into())
            .spawn(move || worker(data, registry, seed, receiver))
            .map_err(|error| anyhow::anyhow!("starting worldgen thread: {error}"))?;
        Ok(Self { jobs })
    }

    /// Generates a full column (noise, surface rules and carvers) off the region threads.
    pub fn generate(&self, x: i32, z: i32) -> anyhow::Result<Generated> {
        let (reply, result) = channel();
        self.jobs
            .send(Job::Chunk { x, z, reply })
            .map_err(|_| anyhow::anyhow!("worldgen thread stopped"))?;
        result
            .recv()
            .map_err(|_| anyhow::anyhow!("worldgen reply lost"))?
            .map_err(anyhow::Error::msg)
    }

    /// The first free block above the generated surface at `(x, z)`.
    pub fn spawn_height(&self, x: i32, z: i32) -> anyhow::Result<i32> {
        let (reply, result) = channel();
        self.jobs
            .send(Job::Height { x, z, reply })
            .map_err(|_| anyhow::anyhow!("worldgen thread stopped"))?;
        result
            .recv()
            .map_err(|_| anyhow::anyhow!("worldgen reply lost"))?
            .map_err(anyhow::Error::msg)
    }
}

fn worker(data: WorldgenData, registry: RuntimeRegistry, seed: i64, jobs: Receiver<Job>) {
    let noise = match NoiseChunkGenerator::new(&data, seed) {
        Ok(noise) => noise,
        Err(error) => {
            for job in jobs {
                let message = format!("creating the overworld generator: {error}");
                match job {
                    Job::Chunk { reply, .. } => {
                        let _ = reply.send(Err(message));
                    }
                    Job::Height { reply, .. } => {
                        let _ = reply.send(Err(message));
                    }
                }
            }
            return;
        }
    };
    for job in jobs {
        match job {
            Job::Chunk { x, z, reply } => {
                let _ = reply.send(generate(&data, &registry, &noise, seed, x, z));
            }
            Job::Height { x, z, reply } => {
                let result = generate(&data, &registry, &noise, seed, x, z).map(|column| {
                    let local_x = x.rem_euclid(16) as u8;
                    let local_z = z.rem_euclid(16) as u8;
                    let height = column
                        .chunk
                        .height(HeightmapKind::WorldSurface, local_x, local_z)
                        .unwrap_or(0) as i32;
                    let y = column.chunk.min_y() + height;
                    if height == 0 {
                        column.chunk.min_y() + column.chunk.height_blocks() as i32 / 2
                    } else {
                        y
                    }
                });
                let _ = reply.send(result);
            }
        }
    }
}

fn generate(
    data: &WorldgenData,
    registry: &RuntimeRegistry,
    noise: &NoiseChunkGenerator,
    seed: i64,
    x: i32,
    z: i32,
) -> Result<Generated, String> {
    let mut chunk = noise
        .fill_noise_chunk(x, z, registry)
        .map_err(|error| format!("noise fill for chunk {x},{z}: {error}"))?;
    let surface = SurfaceGenerator::new(data, noise, seed)
        .map_err(|error| format!("surface rules for chunk {x},{z}: {error}"))?;
    surface
        .apply(&mut chunk, x, z, registry)
        .map_err(|error| format!("surface for chunk {x},{z}: {error}"))?;
    noise
        .carve_chunk(&mut chunk, x, z, registry)
        .map_err(|error| format!("carving chunk {x},{z}: {error}"))?;
    fill_biomes(&mut chunk, &surface, registry, x, z)
        .map_err(|error| format!("biomes for chunk {x},{z}: {error}"))?;
    let mut features =
        FeatureGenerator::new(data, seed, chunk.min_y(), chunk.height_blocks() as i32);
    features
        .decorate(
            &mut chunk,
            x,
            z,
            &|x, y, z| surface.biome_at(x, y, z),
            registry,
        )
        .map_err(|error| format!("features for chunk {x},{z}: {error}"))?;
    let light = sky_light(&chunk, registry);
    Ok(Generated { chunk, light })
}

/// Writes vanilla `MultiNoiseBiomeSource` selections into the chunk's 4x4x4 biome cells.
fn fill_biomes(
    chunk: &mut Chunk,
    surface: &SurfaceGenerator<'_>,
    registry: &RuntimeRegistry,
    chunk_x: i32,
    chunk_z: i32,
) -> Result<(), String> {
    let min_y = chunk.min_y();
    for (section_index, section) in chunk.sections_mut().iter_mut().enumerate() {
        for qy in 0..4 {
            let y = min_y + section_index as i32 * 16 + qy * 4 + 2;
            for qz in 0..4 {
                let z = chunk_z * 16 + qz * 4 + 2;
                for qx in 0..4 {
                    let x = chunk_x * 16 + qx * 4 + 2;
                    let name = surface.biome_at(x, y, z);
                    let id = registry
                        .biome_id(&name.to_string())
                        .ok_or_else(|| format!("unknown biome {name}"))?;
                    section
                        .set_biome((qy * 16 + qz * 4 + qx) as usize, id)
                        .map_err(|error| format!("setting biome: {error}"))?;
                }
            }
        }
    }
    Ok(())
}

/// A conservative sky-light pass: 15 from the top, one level lost per water or
/// leaf block, and zero below the first opaque block. Vanilla also propagates
/// light horizontally; the client's first relight smooths the section edges.
fn sky_light(chunk: &Chunk, registry: &RuntimeRegistry) -> LightData {
    let min_y = chunk.min_y();
    let height = chunk.height_blocks() as usize;
    let mut values = vec![0u8; height * 256];
    for z in 0..16u8 {
        for x in 0..16u8 {
            let mut level = 15u8;
            for y in (min_y..min_y + height as i32).rev() {
                let state = chunk.block(x, y, z).unwrap_or(0);
                let flags = registry.flags(state);
                if flags.motion_blocking && !flags.leaves {
                    level = 0;
                } else if flags.fluid || flags.leaves {
                    level = level.saturating_sub(1);
                }
                let index = (y - min_y) as usize * 256 + usize::from(z) * 16 + usize::from(x);
                values[index] = level;
            }
        }
    }
    let count = chunk.sections().len() + 2;
    let mut sky = Vec::with_capacity(count);
    sky.push(LightSection::Empty);
    for section in 0..chunk.sections().len() {
        let mut data = Box::new([0u8; 2048]);
        let mut any = false;
        for local_y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    let value = values[(section * 16 + local_y) * 256 + z * 16 + x];
                    if value == 0 {
                        continue;
                    }
                    any = true;
                    let index = local_y * 256 + z * 16 + x;
                    data[index / 2] |= value << ((index & 1) * 4);
                }
            }
        }
        sky.push(if any {
            LightSection::Data(data)
        } else {
            LightSection::Empty
        });
    }
    sky.push(LightSection::Empty);
    LightData {
        sky,
        block: vec![LightSection::Empty; count],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_overworld_terrain_with_surface_and_biomes() {
        let terrain = match VanillaTerrain::spawn(0) {
            Ok(terrain) => terrain,
            Err(error) => {
                eprintln!("skipping worldgen test: {error}");
                return;
            }
        };
        let generated = terrain.generate(0, 0).unwrap();
        let registry = RuntimeRegistry::load(vanilla_root()).unwrap();
        let bedrock = registry
            .block_id("minecraft:bedrock", &Default::default())
            .unwrap();
        let mut saw_solid = false;
        let mut saw_bedrock = false;
        let mut names = Vec::new();
        for y in -64..=-40 {
            let block = generated.chunk.block(8, y, 8).unwrap();
            saw_solid |= registry.flags(block).motion_blocking;
            saw_bedrock |= block == bedrock;
            names.push(
                registry
                    .block_state(block)
                    .map_or("unknown", |state| state.name.as_str())
                    .to_owned(),
            );
        }
        assert!(saw_solid, "generated column has no solid blocks: {names:?}");
        assert!(
            saw_bedrock,
            "generated column has no bedrock floor: {names:?}"
        );
        let surface = generated
            .chunk
            .height(HeightmapKind::WorldSurface, 8, 8)
            .unwrap();
        assert!(surface > 0, "generated column has no surface");
        assert_eq!(
            generated.light.sky.len(),
            generated.chunk.sections().len() + 2
        );
        let biome = generated.chunk.sections()[0].biomes().get(0).unwrap();
        assert!(registry.biome_name(biome).is_some());
    }
}
