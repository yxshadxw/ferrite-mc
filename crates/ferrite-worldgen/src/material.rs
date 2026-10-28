//! `worldgen/material_rule` and `worldgen/material_condition` (26.3's surface rules).
//!
//! Noise settings reference one named material rule (`material_rule`), whose tree may in turn
//! reference named rules and conditions. This module models the shape; callers inline the
//! references through [`crate::WorldgenData`] when they evaluate terrain.

use serde::{Deserialize, Serialize};

use crate::density::DensityFunction;
use crate::id::ResourceId;

/// A surface rule: a named reference or an inline operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MaterialRule {
    Reference(ResourceId),
    Node(MaterialRuleNode),
}

/// One inline material rule operation, tagged by `type`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MaterialRuleNode {
    #[serde(rename = "minecraft:sequence")]
    Sequence { sequence: Vec<MaterialRule> },
    #[serde(rename = "minecraft:condition")]
    Condition {
        if_true: MaterialCondition,
        then_run: Box<MaterialRule>,
    },
    #[serde(rename = "minecraft:block")]
    Block { result_state: ResourceId },
    /// Vanilla's spelling of "badlands" in the data format.
    #[serde(rename = "minecraft:bandlands")]
    Bandlands,
    #[serde(rename = "minecraft:ore_vein")]
    OreVein {
        density: Box<DensityFunction>,
        filler_block: ResourceId,
        filler_gap: Box<DensityFunction>,
        ore_block: ResourceId,
        raw_ore_block: ResourceId,
        raw_ore_chance: f32,
        richness: Box<DensityFunction>,
    },
}

/// A surface condition: a named reference or an inline operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MaterialCondition {
    Reference(ResourceId),
    Node(MaterialConditionNode),
}

/// One inline material condition, tagged by `type`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MaterialConditionNode {
    #[serde(rename = "minecraft:above_preliminary_surface")]
    AbovePreliminarySurface,
    #[serde(rename = "minecraft:biome")]
    Biome { biome_is: BiomeSet },
    #[serde(rename = "minecraft:hole")]
    Hole,
    #[serde(rename = "minecraft:noise_threshold")]
    NoiseThreshold {
        noise: ResourceId,
        min_threshold: f64,
        max_threshold: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_3d: Option<bool>,
    },
    #[serde(rename = "minecraft:not")]
    Not { invert: Box<MaterialCondition> },
    #[serde(rename = "minecraft:steep")]
    Steep,
    #[serde(rename = "minecraft:vertical_gradient")]
    VerticalGradient {
        true_at_and_below: VerticalAnchor,
        false_at_and_above: VerticalAnchor,
        random_name: ResourceId,
    },
    #[serde(rename = "minecraft:y_above")]
    YAbove {
        anchor: VerticalAnchor,
        surface_depth_multiplier: i32,
        add_stone_depth: bool,
    },
    #[serde(rename = "minecraft:stone_depth")]
    StoneDepth {
        offset: i32,
        surface_type: SurfaceType,
        secondary_depth_range: i32,
        add_surface_depth: bool,
    },
    #[serde(rename = "minecraft:water")]
    Water {
        offset: i32,
        surface_depth_multiplier: i32,
        add_stone_depth: bool,
    },
}

/// One biome id or a list of biome ids to match.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BiomeSet {
    Single(ResourceId),
    Multiple(Vec<ResourceId>),
}

impl BiomeSet {
    pub fn as_slice(&self) -> &[ResourceId] {
        match self {
            Self::Single(id) => std::slice::from_ref(id),
            Self::Multiple(ids) => ids,
        }
    }
}

/// A vertical position expressed relative to the world or the dimension's ceiling/floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VerticalAnchor {
    Absolute { absolute: i32 },
    AboveBottom { above_bottom: i32 },
    BelowTop { below_top: i32 },
}

/// Whether `stone_depth` measures from the floor or the ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceType {
    Floor,
    Ceiling,
}

#[cfg(test)]
mod tests {
    use super::{MaterialCondition, MaterialConditionNode, MaterialRule, MaterialRuleNode};
    use crate::id::ResourceId;

    #[test]
    fn round_trips_a_surface_rule_tree() {
        let json = r#"{
            "type": "minecraft:sequence",
            "sequence": [
                "minecraft:bedrock_floor",
                {
                    "type": "minecraft:condition",
                    "if_true": {
                        "type": "minecraft:not",
                        "invert": {
                            "type": "minecraft:stone_depth",
                            "add_surface_depth": true,
                            "offset": 0,
                            "secondary_depth_range": 0,
                            "surface_type": "floor"
                        }
                    },
                    "then_run": {
                        "type": "minecraft:block",
                        "result_state": "minecraft:grass_block"
                    }
                }
            ]
        }"#;
        let rule: MaterialRule = serde_json::from_str(json).unwrap();
        let back: MaterialRule =
            serde_json::from_str(&serde_json::to_string(&rule).unwrap()).unwrap();
        assert_eq!(back, rule);

        let MaterialRule::Node(MaterialRuleNode::Sequence { sequence }) = &rule else {
            panic!("expected a sequence");
        };
        assert!(matches!(sequence[0], MaterialRule::Reference(_)));
        let MaterialRule::Node(MaterialRuleNode::Condition { if_true, .. }) = &sequence[1] else {
            panic!("expected a condition");
        };
        assert!(matches!(
            if_true,
            MaterialCondition::Node(MaterialConditionNode::Not { .. })
        ));
        assert_eq!(
            MaterialRule::Reference(ResourceId::parse("minecraft:bedrock_floor").unwrap()),
            sequence[0]
        );
    }
}
