//! Multi-noise biome sources and the generated `reports/biome_parameters` lists.

use serde::{Deserialize, Serialize};

use crate::id::ResourceId;

/// A `worldgen/multi_noise_biome_source_parameter_list` entry.
///
/// The actual parameter list is emitted separately by the data generator under
/// `reports/biome_parameters/<namespace>/<path>.json`; [`crate::WorldgenData`] loads both.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MultiNoisePreset {
    pub preset: ResourceId,
}

/// A generated multi-noise parameter list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MultiNoiseParameters {
    pub biomes: Vec<MultiNoiseBiomeEntry>,
}

/// One biome with its seven climate parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MultiNoiseBiomeEntry {
    pub biome: ResourceId,
    pub parameters: BiomeParameterPoint,
}

/// The climate point (or range) of a biome in the multi-noise parameter space.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BiomeParameterPoint {
    pub continentalness: ParameterRange,
    pub depth: ParameterRange,
    pub erosion: ParameterRange,
    pub humidity: ParameterRange,
    pub offset: ParameterRange,
    pub temperature: ParameterRange,
    pub weirdness: ParameterRange,
}

/// A single value or an inclusive `[min, max]` range.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParameterRange {
    Value(f64),
    Range([f64; 2]),
}

impl ParameterRange {
    pub fn min(self) -> f64 {
        match self {
            Self::Value(value) => value,
            Self::Range([min, _]) => min,
        }
    }

    pub fn max(self) -> f64 {
        match self {
            Self::Value(value) => value,
            Self::Range([_, max]) => max,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MultiNoiseParameters, ParameterRange};

    #[test]
    fn round_trips_ranges_and_scalars() {
        let json = r#"{
            "biomes": [
                {
                    "biome": "minecraft:plains",
                    "parameters": {
                        "continentalness": [-0.11, 1.0],
                        "depth": 0.0,
                        "erosion": [-1.0, 1.0],
                        "humidity": [-1.0, 1.0],
                        "offset": 0.0,
                        "temperature": [-0.45, 0.2],
                        "weirdness": [-1.0, 1.0]
                    }
                }
            ]
        }"#;
        let parameters: MultiNoiseParameters = serde_json::from_str(json).unwrap();
        let entry = &parameters.biomes[0];
        assert_eq!(entry.parameters.depth, ParameterRange::Value(0.0));
        assert_eq!(entry.parameters.continentalness.min(), -0.11);
        assert_eq!(entry.parameters.continentalness.max(), 1.0);
        let back: MultiNoiseParameters =
            serde_json::from_str(&serde_json::to_string(&parameters).unwrap()).unwrap();
        assert_eq!(back, parameters);
    }
}
