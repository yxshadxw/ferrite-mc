//! `worldgen/noise_settings`: noise sampling, noise router, aquifers, and spawn targets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::density::DensityFunction;
use crate::id::ResourceId;

/// A complete noise generator configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoiseSettings {
    /// Present only for dimensions that use aquifers (overworld, amplified, large biomes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aquifers: Option<AquiferSettings>,
    #[serde(default)]
    pub debug_functions: Vec<DebugFunction>,
    pub default_block: ResourceId,
    pub default_fluid: ResourceId,
    pub disable_mob_generation: bool,
    pub legacy_random_source: bool,
    /// Reference to the 26.3 `worldgen/material_rule` registry entry (the surface rule tree).
    pub material_rule: ResourceId,
    pub noise: NoiseSamplingSettings,
    pub noise_router: NoiseRouter,
    pub sea_level: i32,
    #[serde(default)]
    pub spawn_target: Vec<BTreeMap<ResourceId, [f64; 2]>>,
}

/// Vertical extent and cell size of the noise grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoiseSamplingSettings {
    pub height: i32,
    pub min_y: i32,
}

/// The eight density functions of the 26.3 noise router.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoiseRouter {
    pub chunk_surface_level: DensityFunction,
    pub continents: DensityFunction,
    pub depth: DensityFunction,
    pub erosion: DensityFunction,
    pub final_density: DensityFunction,
    pub ridges: DensityFunction,
    pub temperature: DensityFunction,
    pub vegetation: DensityFunction,
}

impl NoiseRouter {
    /// Total number of density-function values across all router fields.
    pub fn node_count(&self) -> usize {
        self.chunk_surface_level.node_count()
            + self.continents.node_count()
            + self.depth.node_count()
            + self.erosion.node_count()
            + self.final_density.node_count()
            + self.ridges.node_count()
            + self.temperature.node_count()
            + self.vegetation.node_count()
    }

    /// Number of unresolved named references across all router fields.
    pub fn reference_count(&self) -> usize {
        self.chunk_surface_level.reference_count()
            + self.continents.reference_count()
            + self.depth.reference_count()
            + self.erosion.reference_count()
            + self.final_density.reference_count()
            + self.ridges.reference_count()
            + self.temperature.reference_count()
            + self.vegetation.reference_count()
    }
}

/// Aquifer noise and exclusion functions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AquiferSettings {
    pub barrier: DensityFunction,
    pub fluid_level_floodedness: DensityFunction,
    pub fluid_level_spread: DensityFunction,
    pub lava: DensityFunction,
    pub surface_level: DensityFunction,
    pub exclusion: DensityFunction,
}

/// One of the debug density functions exposed by `/debug` style tooling.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DebugFunction {
    pub function: DensityFunction,
    pub label: String,
}
