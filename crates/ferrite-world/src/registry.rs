use crate::{BlockFlags, BlockInfo};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockState {
    pub name: String,
    pub properties: BTreeMap<String, String>,
}
pub trait BlockStateRegistry: BlockInfo {
    fn block_id(&self, name: &str, properties: &BTreeMap<String, String>) -> Option<u32>;
    fn block_state(&self, id: u32) -> Option<&BlockState>;
    fn biome_id(&self, name: &str) -> Option<u32>;
    fn biome_name(&self, id: u32) -> Option<&str>;
    fn entity_id(&self, name: &str) -> Option<u32>;
    fn entity_name(&self, id: u32) -> Option<&str>;
    fn block_bits(&self) -> u8;
    fn biome_bits(&self) -> u8;
}
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("invalid registry data: {0}")]
    Invalid(&'static str),
}
#[derive(Debug)]
pub struct RuntimeRegistry {
    states: Vec<Option<BlockState>>,
    lookup: HashMap<BlockState, u32>,
    defaults: HashMap<String, u32>,
    biomes: Vec<String>,
    biome_ids: HashMap<String, u32>,
    entities: HashMap<String, u32>,
    entity_names: HashMap<u32, String>,
    motion: HashSet<String>,
    leaves: HashSet<String>,
}
impl RuntimeRegistry {
    pub fn load(base: impl AsRef<Path>) -> Result<Self, RegistryError> {
        let base = base.as_ref();
        let blocks: serde_json::Value =
            serde_json::from_reader(std::fs::File::open(base.join("reports/blocks.json"))?)?;
        let mut states = Vec::new();
        let mut lookup = HashMap::new();
        let mut defaults = HashMap::new();
        for (name, block) in blocks
            .as_object()
            .ok_or(RegistryError::Invalid("blocks object"))?
        {
            for state in block["states"]
                .as_array()
                .ok_or(RegistryError::Invalid("states array"))?
            {
                let id = state["id"]
                    .as_u64()
                    .and_then(|x| u32::try_from(x).ok())
                    .ok_or(RegistryError::Invalid("state id"))?;
                let mut properties = BTreeMap::new();
                if let Some(map) = state["properties"].as_object() {
                    for (key, val) in map {
                        properties.insert(
                            key.clone(),
                            val.as_str()
                                .ok_or(RegistryError::Invalid("state property"))?
                                .to_owned(),
                        );
                    }
                }
                let entry = BlockState {
                    name: name.clone(),
                    properties,
                };
                if states.len() <= id as usize {
                    states.resize(id as usize + 1, None);
                }
                if states[id as usize].replace(entry.clone()).is_some() {
                    return Err(RegistryError::Invalid("duplicate state"));
                }
                lookup.insert(entry, id);
                if state["default"].as_bool() == Some(true) {
                    defaults.insert(name.clone(), id);
                }
            }
        }
        let mut biomes: Vec<String> =
            std::fs::read_dir(base.join("data/minecraft/worldgen/biome"))?
                .map(|e| {
                    let e = e?;
                    Ok(format!(
                        "minecraft:{}",
                        e.path()
                            .file_stem()
                            .ok_or(RegistryError::Invalid("biome filename"))?
                            .to_string_lossy()
                    ))
                })
                .collect::<Result<_, RegistryError>>()?;
        biomes.sort();
        let biome_ids = biomes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i as u32))
            .collect();
        let registries: serde_json::Value =
            serde_json::from_reader(std::fs::File::open(base.join("reports/registries.json"))?)?;
        let mut entities = HashMap::new();
        let mut entity_names = HashMap::new();
        for (name, entry) in registries["minecraft:block_entity_type"]["entries"]
            .as_object()
            .ok_or(RegistryError::Invalid("block entity registry"))?
        {
            let id = entry["protocol_id"]
                .as_u64()
                .and_then(|id| u32::try_from(id).ok())
                .ok_or(RegistryError::Invalid("block entity ID"))?;
            entities.insert(name.clone(), id);
            entity_names.insert(id, name.clone());
        }
        let tag_dir = base.join("data/minecraft/tags/block");
        let motion = load_tag(
            &tag_dir,
            "minecraft:blocks_motion_in_heightmap",
            &mut HashSet::new(),
        )?;
        let leaves = load_tag(&tag_dir, "minecraft:leaves", &mut HashSet::new())?;
        Ok(Self {
            motion,
            leaves,
            states,
            lookup,
            defaults,
            biomes,
            biome_ids,
            entities,
            entity_names,
        })
    }
}
impl BlockStateRegistry for RuntimeRegistry {
    fn block_id(&self, name: &str, properties: &BTreeMap<String, String>) -> Option<u32> {
        if properties.is_empty() {
            self.defaults.get(name).copied()
        } else {
            self.lookup
                .get(&BlockState {
                    name: name.into(),
                    properties: properties.clone(),
                })
                .copied()
        }
    }
    fn block_state(&self, id: u32) -> Option<&BlockState> {
        self.states.get(id as usize)?.as_ref()
    }
    fn biome_id(&self, name: &str) -> Option<u32> {
        self.biome_ids.get(name).copied()
    }
    fn biome_name(&self, id: u32) -> Option<&str> {
        self.biomes.get(id as usize).map(String::as_str)
    }
    fn entity_id(&self, name: &str) -> Option<u32> {
        self.entities.get(name).copied()
    }
    fn entity_name(&self, id: u32) -> Option<&str> {
        self.entity_names.get(&id).map(String::as_str)
    }
    fn block_bits(&self) -> u8 {
        ((usize::BITS - self.states.len().saturating_sub(1).leading_zeros()) as u8).max(9)
    }
    fn biome_bits(&self) -> u8 {
        ((usize::BITS - self.biomes.len().saturating_sub(1).leading_zeros()) as u8).max(4)
    }
}
/// Heightmap motion and leaves use generated vanilla block tags; aquatic fluid
/// state uses the known vanilla plant and waterlogged representations until
/// an authoritative per-state table is supplied through `BlockInfo`.
impl BlockInfo for RuntimeRegistry {
    fn flags(&self, id: u32) -> BlockFlags {
        let Some(state) = self.block_state(id) else {
            return BlockFlags::default();
        };
        let name = state.name.strip_prefix("minecraft:").unwrap_or(&state.name);
        let air = matches!(name, "air" | "cave_air" | "void_air");
        let fluid = matches!(
            name,
            "water"
                | "lava"
                | "seagrass"
                | "tall_seagrass"
                | "kelp"
                | "kelp_plant"
                | "bubble_column"
        ) || state
            .properties
            .get("waterlogged")
            .is_some_and(|v| v == "true");
        BlockFlags {
            air,
            fluid,
            leaves: self.leaves.contains(&state.name),
            motion_blocking: self.motion.contains(&state.name),
        }
    }
}
fn load_tag(
    dir: &Path,
    name: &str,
    active: &mut HashSet<String>,
) -> Result<HashSet<String>, RegistryError> {
    if !active.insert(name.to_owned()) {
        return Err(RegistryError::Invalid("cyclic block tag"));
    }
    let local = name
        .strip_prefix("minecraft:")
        .ok_or(RegistryError::Invalid("non-vanilla block tag"))?;
    let data: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(dir.join(format!("{local}.json")))?)?;
    let mut result = HashSet::new();
    for value in data["values"]
        .as_array()
        .ok_or(RegistryError::Invalid("block tag values"))?
    {
        let value = value
            .as_str()
            .or_else(|| value.get("id").and_then(|v| v.as_str()))
            .ok_or(RegistryError::Invalid("block tag value"))?;
        if let Some(tag) = value.strip_prefix('#') {
            result.extend(load_tag(dir, tag, active)?);
        } else {
            result.insert(value.to_owned());
        }
    }
    active.remove(name);
    Ok(result)
}
