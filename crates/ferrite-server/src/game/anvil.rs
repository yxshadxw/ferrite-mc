use ferrite_nbt::{List, Tag, binary};
use ferrite_world::{
    BlockStateRegistry, Chunk, LightData, LightSection, RegionFile, RuntimeRegistry,
    chunk_from_nbt, encode_chunk_packet,
};
use std::path::Path;

#[cfg(test)]
pub fn read_chunk(
    world: &Path,
    registry: &RuntimeRegistry,
    x: i32,
    z: i32,
) -> anyhow::Result<Option<Vec<u8>>> {
    Ok(read_column(world, registry, x, z)?
        .map(|(chunk, light)| encode_chunk_packet(x, z, &chunk, &light))
        .transpose()?)
}
fn read_column(
    world: &Path,
    registry: &RuntimeRegistry,
    x: i32,
    z: i32,
) -> anyhow::Result<Option<(Chunk, LightData)>> {
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
    Ok(Some((chunk, light)))
}

use ferrite_protocol::{Encode, Packet, VarInt, packets::play::ChunkData};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

const CACHE_LIMIT: usize = 4096;
const PENDING_LIMIT: usize = 128;
enum Entry {
    Pending(u64),
    Ready(Result<Box<Column>, String>, u64),
}
#[derive(Clone)]
struct Column {
    chunk: Chunk,
    light: LightData,
    packet: Vec<u8>,
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
    path: Option<PathBuf>,
    registry: Arc<RuntimeRegistry>,
    cache: Mutex<Cache>,
    runtime: tokio::runtime::Handle,
    flush_lock: tokio::sync::Mutex<()>,
    flat: Mutex<HashMap<i32, (Chunk, LightData)>>,
}
impl AnvilChunkSource {
    pub fn new(path: PathBuf, registry: RuntimeRegistry) -> Arc<Self> {
        Arc::new(Self {
            path: Some(path),
            registry: Arc::new(registry),
            cache: Mutex::new(Cache {
                entries: HashMap::new(),
                clock: 0,
                dirty: HashMap::new(),
            }),
            runtime: tokio::runtime::Handle::current(),
            flush_lock: tokio::sync::Mutex::new(()),
            flat: Mutex::new(HashMap::new()),
        })
    }
    pub fn new_memory() -> Arc<Self> {
        static REGISTRY: OnceLock<Arc<RuntimeRegistry>> = OnceLock::new();
        let registry = REGISTRY
            .get_or_init(|| {
                Arc::new(
                    RuntimeRegistry::load(
                        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../vanilla/26.3/generated"),
                    )
                    .expect("loading vanilla registry"),
                )
            })
            .clone();
        Arc::new(Self {
            path: None,
            registry,
            cache: Mutex::new(Cache {
                entries: HashMap::new(),
                clock: 0,
                dirty: HashMap::new(),
            }),
            runtime: tokio::runtime::Handle::current(),
            flush_lock: tokio::sync::Mutex::new(()),
            flat: Mutex::new(HashMap::new()),
        })
    }
    pub fn modified_packet(&self, x: i32, z: i32) -> Option<Vec<u8>> {
        self.cache
            .lock()
            .unwrap()
            .dirty
            .get(&(x, z))
            .map(|dirty| dirty.packet.clone())
    }
    pub fn block(&self, x: i32, y: i32, z: i32) -> Option<u32> {
        let key = (x.div_euclid(16), z.div_euclid(16));
        let cache = self.cache.lock().unwrap();
        if let Some(d) = cache.dirty.get(&key) {
            return d
                .chunk
                .block(x.rem_euclid(16) as u8, y, z.rem_euclid(16) as u8);
        }
        let column = cache.entries.iter().find_map(|(&(cx, cz, _), entry)| {
            if (cx, cz) == key {
                if let Entry::Ready(Ok(c), _) = entry {
                    Some(c)
                } else {
                    None
                }
            } else {
                None
            }
        })?;
        column
            .chunk
            .block(x.rem_euclid(16) as u8, y, z.rem_euclid(16) as u8)
    }
    pub fn set_block(&self, x: i32, y: i32, z: i32, state: u32) -> anyhow::Result<()> {
        self.compare_set_block(x, y, z, None, state)
    }
    pub fn compare_set_block(
        &self,
        x: i32,
        y: i32,
        z: i32,
        expected: Option<u32>,
        state: u32,
    ) -> anyhow::Result<()> {
        let key = (x.div_euclid(16), z.div_euclid(16));
        let mut cache = self.cache.lock().unwrap();
        let (mut chunk, light) = if let Some(d) = cache.dirty.get(&key) {
            (d.chunk.clone(), d.light.clone())
        } else {
            let column = cache
                .entries
                .iter()
                .find_map(|(&(cx, cz, _), entry)| {
                    if (cx, cz) == key {
                        if let Entry::Ready(Ok(c), _) = entry {
                            Some(c)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .ok_or_else(|| anyhow::anyhow!("chunk not loaded"))?;
            (column.chunk.clone(), column.light.clone())
        };
        anyhow::ensure!(
            self.path.is_none()
                || cache.dirty.contains_key(&key)
                || cache.dirty.len() < CACHE_LIMIT,
            "dirty chunk cache is full"
        );
        if let Some(expected) = expected {
            anyhow::ensure!(
                chunk.block(x.rem_euclid(16) as u8, y, z.rem_euclid(16) as u8) == Some(expected),
                "block changed"
            );
        }
        chunk.set_block(
            x.rem_euclid(16) as u8,
            y,
            z.rem_euclid(16) as u8,
            state,
            &*self.registry,
        )?;
        let body = encode_chunk_packet(key.0, key.1, &chunk, &light)?;
        let mut packet = Vec::new();
        VarInt(ChunkData::ID).encode(&mut packet);
        packet.extend(body);
        cache.clock += 1;
        let version = cache.clock;
        cache.entries.retain(|&(cx, cz, _), _| (cx, cz) != key);
        cache.dirty.insert(
            key,
            Dirty {
                chunk,
                light,
                packet,
                version,
            },
        );
        Ok(())
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
            self.path.is_none()
                || cache.dirty.contains_key(&(x, z))
                || cache.dirty.len() < CACHE_LIMIT,
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
        if self.path.is_none() {
            return Ok(());
        }
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
            let dir = source
                .path
                .as_ref()
                .unwrap()
                .join("dimensions/minecraft/overworld/region");
            std::fs::create_dir_all(&dir)?;
            let rx = x.div_euclid(32);
            let rz = z.div_euclid(32);
            let region = RegionFile::open(dir.join(format!("r.{rx}.{rz}.mca")), rx, rz)?;
            let mut tag = ferrite_world::chunk_to_nbt(&chunk, &*source.registry)?;
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
        if self.path.is_none() {
            return Ok(());
        }
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
                    Some(
                        data.as_ref()
                            .map(|column| column.packet.clone())
                            .map_err(Clone::clone),
                    )
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
            let result = (|| -> anyhow::Result<Column> {
                let loaded = match &source.path { Some(path) => read_column(path, &source.registry, x, z)?, None => None };
                let (chunk, light) = match loaded { Some(column) => column, None => { let mut flat = source.flat.lock().unwrap(); if let Some(column) = flat.get(&biome) { column.clone() } else { let column = (flat_chunk(&source.registry, biome)?, LightData { sky: vec![LightSection::Empty;26], block: vec![LightSection::Empty;26] }); flat.insert(biome, column.clone()); column } } };
                let body = encode_chunk_packet(x, z, &chunk, &light)?;
                let mut packet = Vec::new(); VarInt(ChunkData::ID).encode(&mut packet); packet.extend(body);
                Ok(Column { chunk, light, packet })
            })().map_err(|e| format!("{e:#}"));
            let mut cache = source.cache.lock().unwrap();
            cache.clock += 1;
            let now = cache.clock;
            if matches!(cache.entries.get(&key), Some(Entry::Pending(version)) if *version == request) { cache.entries.insert(key, Entry::Ready(result.map(Box::new), now)); }
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

fn flat_chunk(registry: &RuntimeRegistry, biome: i32) -> anyhow::Result<Chunk> {
    let air = registry
        .block_id("minecraft:air", &Default::default())
        .ok_or_else(|| anyhow::anyhow!("missing air"))?;
    let bedrock = registry
        .block_id("minecraft:bedrock", &Default::default())
        .ok_or_else(|| anyhow::anyhow!("missing bedrock"))?;
    let dirt = registry
        .block_id("minecraft:dirt", &Default::default())
        .ok_or_else(|| anyhow::anyhow!("missing dirt"))?;
    let grass = registry
        .block_id("minecraft:grass_block", &Default::default())
        .ok_or_else(|| anyhow::anyhow!("missing grass"))?;
    let mut chunk = Chunk::new(
        -64,
        384,
        air,
        biome as u32,
        registry.block_bits(),
        registry.biome_bits(),
    )?;
    for (section, layer, state) in [
        (7, 13, bedrock),
        (7, 14, dirt),
        (7, 15, dirt),
        (8, 0, grass),
    ] {
        for index in layer * 256..(layer + 1) * 256 {
            chunk.sections_mut()[section].set_block(index, state, registry)?;
        }
    }
    chunk.recompute_heightmaps(registry);
    Ok(chunk)
}
