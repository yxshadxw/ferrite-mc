//! `dimension_type`, `dimension`, and `worldgen/world_preset` entries.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::ResourceId;

/// A dimension type: the world height, light, and visual attributes of a dimension.
///
/// The 26.3 `attributes` map is kept as raw JSON because it is an open, registry-keyed set of
/// gameplay and visual attributes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DimensionType {
    pub ambient_light: f32,
    #[serde(default)]
    pub attributes: BTreeMap<String, Value>,
    pub coordinate_scale: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_clock: Option<ResourceId>,
    pub has_ceiling: bool,
    pub has_ender_dragon_fight: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_fixed_time: Option<bool>,
    pub has_skylight: bool,
    pub height: i32,
    /// Tag id, including the leading `#` (for example `#minecraft:infiniburn_overworld`).
    pub infiniburn: String,
    pub logical_height: i32,
    pub min_y: i32,
    pub monster_spawn_block_light_limit: i32,
    pub monster_spawn_light_level: IntProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skybox: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cardinal_light: Option<String>,
    /// Tag id, including the leading `#`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timelines: Option<String>,
}

/// A dimension definition, either standalone or nested in a world preset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    #[serde(rename = "type")]
    pub dimension_type: ResourceId,
    pub generator: WorldGenerator,
}

/// A named collection of dimensions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldPreset {
    pub dimensions: BTreeMap<ResourceId, Dimension>,
}

/// The chunk generator of a dimension.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorldGenerator {
    #[serde(rename = "minecraft:noise")]
    Noise {
        biome_source: BiomeSource,
        settings: ResourceId,
    },
    #[serde(rename = "minecraft:flat")]
    Flat { settings: FlatSettings },
    #[serde(rename = "minecraft:debug")]
    Debug,
}

/// The biome source of a noise generator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BiomeSource {
    #[serde(rename = "minecraft:multi_noise")]
    MultiNoise { preset: ResourceId },
    #[serde(rename = "minecraft:the_end")]
    TheEnd,
    #[serde(rename = "minecraft:fixed")]
    Fixed { biome: ResourceId },
}

/// Settings of a superflat generator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlatSettings {
    pub biome: ResourceId,
    pub features: bool,
    pub lakes: bool,
    pub layers: Vec<FlatLayer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure_overrides: Option<Vec<ResourceId>>,
}

/// One superflat layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlatLayer {
    pub block: ResourceId,
    pub height: i32,
}

/// A constant integer or a uniform range. Used for monster spawn light levels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IntProvider {
    Constant(i32),
    Uniform(UniformInt),
}

/// A `minecraft:uniform` integer provider.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UniformInt {
    #[serde(rename = "type")]
    pub provider_type: UniformProviderType,
    pub min_inclusive: i32,
    pub max_inclusive: i32,
}

/// The only integer provider seen in 26.3 dimension types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UniformProviderType {
    #[serde(rename = "minecraft:uniform")]
    Uniform,
}

#[cfg(test)]
mod tests {
    use super::{BiomeSource, Dimension, WorldGenerator, WorldPreset};
    use crate::id::ResourceId;
    use std::collections::BTreeMap;

    #[test]
    fn round_trips_a_world_preset() {
        let json = r#"{
            "dimensions": {
                "minecraft:overworld": {
                    "type": "minecraft:overworld",
                    "generator": {
                        "type": "minecraft:noise",
                        "biome_source": {"type": "minecraft:multi_noise", "preset": "minecraft:overworld"},
                        "settings": "minecraft:overworld"
                    }
                },
                "minecraft:the_end": {
                    "type": "minecraft:the_end",
                    "generator": {
                        "type": "minecraft:noise",
                        "biome_source": {"type": "minecraft:the_end"},
                        "settings": "minecraft:end"
                    }
                }
            }
        }"#;
        let preset: WorldPreset = serde_json::from_str(json).unwrap();
        let back: WorldPreset =
            serde_json::from_str(&serde_json::to_string(&preset).unwrap()).unwrap();
        assert_eq!(back, preset);
        assert_eq!(preset.dimensions.len(), 2);
        assert!(matches!(
            preset.dimensions[&ResourceId::parse("overworld").unwrap()].generator,
            WorldGenerator::Noise {
                biome_source: BiomeSource::MultiNoise { .. },
                ..
            }
        ));

        let dimension = Dimension {
            dimension_type: ResourceId::parse("the_end").unwrap(),
            generator: WorldGenerator::Debug,
        };
        let mut dimensions = BTreeMap::new();
        dimensions.insert(ResourceId::parse("end_debug").unwrap(), dimension);
        let preset = WorldPreset { dimensions };
        let json = serde_json::to_string(&preset).unwrap();
        assert!(json.contains("\"minecraft:the_end\""));
    }
}
