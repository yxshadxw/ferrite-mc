use ferrite_nbt::{List, Tag, binary};
use ferrite_world::{
    LightData, LightSection, RegionFile, RuntimeRegistry, chunk_from_nbt, encode_chunk_packet,
};
use std::path::Path;

pub fn read_chunk(
    world: &Path,
    registry: &RuntimeRegistry,
    x: i32,
    z: i32,
) -> anyhow::Result<Option<Vec<u8>>> {
    let rx = x.div_euclid(32);
    let rz = z.div_euclid(32);
    let dir = world.join("dimensions/minecraft/overworld/region");
    if !dir.exists() {
        return Ok(None);
    }
    let region = RegionFile::open(dir.join(format!("r.{rx}.{rz}.mca")), rx, rz)?;
    let Some(data) = region.read_chunk(x, z)? else {
        return Ok(None);
    };
    let (_, tag) = binary::read_named(&data)?;
    let chunk = chunk_from_nbt(&tag, -64, 384, registry)?;
    let mut light = LightData {
        sky: vec![LightSection::Unknown; 26],
        block: vec![LightSection::Unknown; 26],
    };
    if let Tag::Compound(root) = &tag
        && let Some(Tag::List(List::Compound(sections))) = root.get("sections")
    {
        for section in sections {
            let Some(Tag::Byte(y)) = section.get("Y") else {
                continue;
            };
            let idx = i32::from(*y) + 5;
            if !(0..26).contains(&idx) {
                continue;
            }
            for (name, target) in [
                ("SkyLight", &mut light.sky),
                ("BlockLight", &mut light.block),
            ] {
                if let Some(Tag::ByteArray(bytes)) = section.get(name) {
                    anyhow::ensure!(bytes.len() == 2048, "invalid {name} length");
                    let mut array = Box::new([0; 2048]);
                    for (dst, src) in array.iter_mut().zip(bytes) {
                        *dst = *src as u8;
                    }
                    target[idx as usize] = LightSection::Data(array);
                }
            }
        }
    }
    Ok(Some(encode_chunk_packet(x, z, &chunk, &light)?))
}

use super::chunk::{ChunkSource, FlatGenerator};
use ferrite_protocol::{Encode, Packet, VarInt, packets::play::ChunkData};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

const CACHE_LIMIT: usize = 256;
const PENDING_LIMIT: usize = 32;
enum Entry {
    Pending(u64),
    Ready(Result<Vec<u8>, String>, u64),
}
struct Cache {
    entries: HashMap<(i32, i32, i32), Entry>,
    clock: u64,
    dirty: HashMap<(i32, i32), Dirty>,
}
struct Dirty {
    chunk: ferrite_world::Chunk,
    light: LightData,
    packet: Vec<u8>,
    version: u64,
}
pub struct AnvilChunkSource {
    path: PathBuf,
    registry: RuntimeRegistry,
    cache: Mutex<Cache>,
    runtime: tokio::runtime::Handle,
    flush_lock: tokio::sync::Mutex<()>,
}
impl AnvilChunkSource {
    pub fn new(path: PathBuf, registry: RuntimeRegistry) -> Arc<Self> {
        Arc::new(Self {
            path,
            registry,
            cache: Mutex::new(Cache {
                entries: HashMap::new(),
                clock: 0,
                dirty: HashMap::new(),
            }),
            runtime: tokio::runtime::Handle::current(),
            flush_lock: tokio::sync::Mutex::new(()),
        })
    }
    /// Records a modified column for streaming and eventual Anvil writeback.
    /// TODO: call this when block-editing gameplay is implemented.
    pub fn mark_modified(
        &self,
        x: i32,
        z: i32,
        chunk: ferrite_world::Chunk,
        light: &LightData,
    ) -> anyhow::Result<()> {
        let body = encode_chunk_packet(x, z, &chunk, light)?;
        let mut packet = Vec::new();
        VarInt(ChunkData::ID).encode(&mut packet);
        packet.extend(body);
        let mut cache = self.cache.lock().unwrap();
        anyhow::ensure!(
            cache.dirty.contains_key(&(x, z)) || cache.dirty.len() < CACHE_LIMIT,
            "dirty chunk cache is full"
        );
        cache.clock += 1;
        let version = cache.clock;
        cache.entries.retain(|&(cx, cz, _), _| (cx, cz) != (x, z));
        cache.dirty.insert(
            (x, z),
            Dirty {
                chunk,
                light: light.clone(),
                packet,
                version,
            },
        );
        Ok(())
    }
    pub async fn flush_chunk(self: &Arc<Self>, x: i32, z: i32) -> anyhow::Result<()> {
        let _guard = self.flush_lock.lock().await;
        let item = {
            let cache = self.cache.lock().unwrap();
            cache
                .dirty
                .get(&(x, z))
                .map(|d| (d.chunk.clone(), d.light.clone(), d.version))
        };
        let Some((chunk, light, version)) = item else {
            return Ok(());
        };
        let source = Arc::clone(self);
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let dir = source.path.join("dimensions/minecraft/overworld/region");
            std::fs::create_dir_all(&dir)?;
            let rx = x.div_euclid(32);
            let rz = z.div_euclid(32);
            let region = RegionFile::open(dir.join(format!("r.{rx}.{rz}.mca")), rx, rz)?;
            let mut tag = ferrite_world::chunk_to_nbt(&chunk, &source.registry)?;
            if let Tag::Compound(root) = &mut tag {
                root.insert("xPos".into(), Tag::Int(x));
                root.insert("zPos".into(), Tag::Int(z));
                if let Some(Tag::List(List::Compound(entities))) = root.get_mut("block_entities") {
                    for entity in entities {
                        for (key, chunk_coord) in [("x", x), ("z", z)] {
                            if let Some(Tag::Int(value)) = entity.get_mut(key) {
                                *value = chunk_coord * 16 + value.rem_euclid(16);
                            }
                        }
                    }
                }
            }
            apply_light(&mut tag, &light)?;
            let bytes = binary::write_named("", &tag)?;
            region.write_chunk(x, z, &bytes, ferrite_world::RegionCompression::Zlib)?;
            Ok(())
        })
        .await??;
        let mut cache = self.cache.lock().unwrap();
        if cache
            .dirty
            .get(&(x, z))
            .is_some_and(|d| d.version == version)
        {
            cache.dirty.remove(&(x, z));
        }
        Ok(())
    }
    pub async fn flush_all(self: &Arc<Self>) -> anyhow::Result<()> {
        loop {
            let next = { self.cache.lock().unwrap().dirty.keys().next().copied() };
            let Some((x, z)) = next else { return Ok(()) };
            self.flush_chunk(x, z).await?;
        }
    }
    pub fn get(self: &Arc<Self>, x: i32, z: i32, biome: i32) -> Option<Result<Vec<u8>, String>> {
        let key = (x, z, biome);
        let mut cache = self.cache.lock().unwrap();
        cache.clock += 1;
        let now = cache.clock;
        if let Some(dirty) = cache.dirty.get(&(x, z)) {
            return Some(Ok(dirty.packet.clone()));
        }
        if let Some(entry) = cache.entries.get_mut(&key) {
            return match entry {
                Entry::Pending(_) => None,
                Entry::Ready(data, used) => {
                    *used = now;
                    Some(data.clone())
                }
            };
        }
        if cache
            .entries
            .values()
            .filter(|e| matches!(e, Entry::Pending(_)))
            .count()
            >= PENDING_LIMIT
        {
            return None;
        }
        if cache.entries.len() >= CACHE_LIMIT {
            let oldest = cache
                .entries
                .iter()
                .filter_map(|(k, v)| {
                    if let Entry::Ready(_, used) = v {
                        Some((*k, *used))
                    } else {
                        None
                    }
                })
                .min_by_key(|(_, used)| *used)
                .map(|(k, _)| k)?;
            cache.entries.remove(&oldest);
        }
        cache.entries.insert(key, Entry::Pending(now));
        drop(cache);
        let source = Arc::clone(self);
        let request = now;
        self.runtime.spawn_blocking(move || {
            let result = read_chunk(&source.path, &source.registry, x, z)
                .map(|packet| match packet {
                    Some(body) => {
                        let mut packet = Vec::new();
                        VarInt(ChunkData::ID).encode(&mut packet);
                        packet.extend(body);
                        packet
                    }
                    None => {
                        let mut generator = FlatGenerator::default();
                        let chunk = generator.get(x, z, biome);
                        let mut packet = Vec::new();
                        ferrite_protocol::encode_packet(&chunk, &mut packet);
                        packet
                    }
                })
                .map_err(|e| format!("{e:#}"));
            let mut cache = source.cache.lock().unwrap();
            cache.clock += 1;
            let now = cache.clock;
            if matches!(cache.entries.get(&key), Some(Entry::Pending(version)) if *version == request) { cache.entries.insert(key, Entry::Ready(result, now)); }
        });
        None
    }
}
pub fn read_spawn(world: &Path) -> anyhow::Result<[i32; 3]> {
    let bytes = std::fs::read(world.join("level.dat"))?;
    let (_, tag) = binary::read_compressed(&bytes, binary::Compression::Gzip)?;
    let Tag::Compound(root) = tag else {
        anyhow::bail!("level.dat root is not a compound")
    };
    let Some(Tag::Compound(data)) = root.get("Data") else {
        anyhow::bail!("level.dat missing Data")
    };
    let Some(Tag::Compound(spawn)) = data.get("spawn") else {
        anyhow::bail!("level.dat missing spawn")
    };
    let Some(Tag::IntArray(pos)) = spawn.get("pos") else {
        anyhow::bail!("level.dat missing spawn position")
    };
    anyhow::ensure!(pos.len() == 3, "invalid spawn position");
    Ok([pos[0], pos[1], pos[2]])
}

fn apply_light(tag: &mut Tag, light: &LightData) -> anyhow::Result<()> {
    let Tag::Compound(root) = tag else {
        anyhow::bail!("invalid chunk root")
    };
    let Some(Tag::List(List::Compound(sections))) = root.get_mut("sections") else {
        anyhow::bail!("missing sections")
    };
    for (name, values) in [("SkyLight", &light.sky), ("BlockLight", &light.block)] {
        for (idx, value) in values.iter().enumerate() {
            let data = match value {
                LightSection::Unknown => continue,
                LightSection::Empty => vec![0; 2048],
                LightSection::Data(bytes) => bytes.iter().map(|b| *b as i8).collect(),
            };
            let y = i8::try_from(idx as i32 - 5)?;
            let section_index = match sections
                .iter()
                .position(|s| s.get("Y") == Some(&Tag::Byte(y)))
            {
                Some(i) => i,
                None => {
                    let mut section = ferrite_nbt::Compound::new();
                    section.insert("Y".into(), Tag::Byte(y));
                    sections.push(section);
                    sections.len() - 1
                }
            };
            sections[section_index].insert(name.into(), Tag::ByteArray(data));
        }
    }
    Ok(())
}
