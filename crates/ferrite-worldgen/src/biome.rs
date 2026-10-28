//! `worldgen/biome` entries.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::id::ResourceId;

/// A biome definition.
///
/// `attributes` is an open, registry-keyed map in 26.3 (spawners, audio, fog, and so on). It
/// is preserved as raw JSON so no attribute is dropped; [`Self::spawner_ids`] extracts the
/// entity ids from the natural-spawn attribute.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Biome {
    #[serde(default)]
    pub attributes: BTreeMap<String, Value>,
    #[serde(default)]
    pub carvers: Carvers,
    pub downfall: f32,
    #[serde(default)]
    pub effects: BiomeEffects,
    #[serde(default)]
    pub features: Vec<Vec<ResourceId>>,
    pub has_precipitation: bool,
    pub temperature: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_modifier: Option<TemperatureModifier>,
}

impl Biome {
    /// Entity ids from `minecraft:gameplay/natural_mob_spawns`, in category order.
    pub fn spawner_ids(&self) -> Vec<&str> {
        let categories = self
            .attributes
            .get("minecraft:gameplay/natural_mob_spawns")
            .and_then(|spawns| spawns.pointer("/argument/spawns_by_category"))
            .and_then(Value::as_object);
        let Some(categories) = categories else {
            return Vec::new();
        };
        categories
            .values()
            .filter_map(Value::as_array)
            .flatten()
            .filter_map(|spawn| spawn.get("type"))
            .filter_map(Value::as_str)
            .collect()
    }
}

/// The carver list. Vanilla writes a single id for biomes with only one carver and an array
/// otherwise; both spellings are preserved so a strict round-trip is lossless.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Carvers {
    Single(ResourceId),
    Multiple(Vec<ResourceId>),
}

impl Default for Carvers {
    fn default() -> Self {
        Self::Multiple(Vec::new())
    }
}

impl Carvers {
    /// The carver ids, whether the source used a single id or a list.
    pub fn as_slice(&self) -> &[ResourceId] {
        match self {
            Self::Single(id) => std::slice::from_ref(id),
            Self::Multiple(ids) => ids,
        }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, ResourceId> {
        self.as_slice().iter()
    }
}

/// Client-side biome effects.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BiomeEffects {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foliage_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grass_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_foliage_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grass_color_modifier: Option<GrassColorModifier>,
}

/// Special grass tinting mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrassColorModifier {
    DarkForest,
    Swamp,
}

/// Temperature behavior for frozen biomes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemperatureModifier {
    Frozen,
}

#[cfg(test)]
mod tests {
    use super::{Biome, GrassColorModifier};
    use crate::id::ResourceId;

    #[test]
    fn round_trips_and_extracts_spawners() {
        let json = r##"{
            "attributes": {
                "minecraft:gameplay/natural_mob_spawns": {
                    "argument": {
                        "spawns_by_category": {
                            "creature": [
                                {"type": "minecraft:sheep", "count": 4, "weight": 12}
                            ]
                        },
                        "spawn_costs": {}
                    },
                    "modifier": "overlay"
                },
                "minecraft:visual/sky_color": "#78a7ff"
            },
            "carvers": ["minecraft:cave"],
            "downfall": 0.4,
            "effects": {"water_color": "#3f76e4", "grass_color_modifier": "swamp"},
            "features": [["minecraft:flower_plains"]],
            "has_precipitation": true,
            "temperature": 0.8,
            "temperature_modifier": "frozen"
        }"##;
        let biome: Biome = serde_json::from_str(json).unwrap();
        assert_eq!(biome.spawner_ids(), ["minecraft:sheep"]);
        assert_eq!(
            biome.effects.grass_color_modifier,
            Some(GrassColorModifier::Swamp)
        );
        assert_eq!(
            biome.carvers.as_slice(),
            [ResourceId::parse("cave").unwrap()]
        );
        let back: Biome = serde_json::from_str(&serde_json::to_string(&biome).unwrap()).unwrap();
        assert_eq!(back, biome);
    }
}
