//! The density function AST used by noise routers, aquifers, and surface rules.

use serde::{Deserialize, Serialize};

use crate::id::ResourceId;

/// A density function: an inline constant, a named reference, or a typed operation.
///
/// References are resolved by [`crate::DensityFunction::resolve_references`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DensityFunction {
    /// A plain JSON number.
    Constant(f64),
    /// A string naming another density function (`minecraft:overworld/final_density`).
    Reference(ResourceId),
    /// An inline operation.
    Node(DensityNode),
}

/// One inline density function operation, tagged by its `type` field.
///
/// The variant names and field names mirror the 26.3 data generator output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DensityNode {
    #[serde(rename = "minecraft:add")]
    Add {
        left: Box<DensityFunction>,
        right: Box<DensityFunction>,
    },
    #[serde(rename = "minecraft:sub")]
    Sub {
        left: Box<DensityFunction>,
        right: Box<DensityFunction>,
    },
    #[serde(rename = "minecraft:mul")]
    Mul {
        left: Box<DensityFunction>,
        right: Box<DensityFunction>,
    },
    #[serde(rename = "minecraft:div")]
    Div {
        left: Box<DensityFunction>,
        right: Box<DensityFunction>,
    },
    #[serde(rename = "minecraft:min")]
    Min {
        left: Box<DensityFunction>,
        right: Box<DensityFunction>,
    },
    #[serde(rename = "minecraft:max")]
    Max {
        left: Box<DensityFunction>,
        right: Box<DensityFunction>,
    },
    #[serde(rename = "minecraft:abs")]
    Abs { input: Box<DensityFunction> },
    #[serde(rename = "minecraft:square")]
    Square { input: Box<DensityFunction> },
    #[serde(rename = "minecraft:cube")]
    Cube { input: Box<DensityFunction> },
    #[serde(rename = "minecraft:half_negative")]
    HalfNegative { input: Box<DensityFunction> },
    #[serde(rename = "minecraft:quarter_negative")]
    QuarterNegative { input: Box<DensityFunction> },
    #[serde(rename = "minecraft:squeeze")]
    Squeeze { input: Box<DensityFunction> },
    #[serde(rename = "minecraft:negate")]
    Negate { input: Box<DensityFunction> },
    #[serde(rename = "minecraft:clamp")]
    Clamp {
        input: Box<DensityFunction>,
        min: f64,
        max: f64,
    },
    /// Vanilla's `y_clamped_gradient`, generalized to any axis in 26.3.
    #[serde(rename = "minecraft:gradient")]
    Gradient {
        axis: Axis,
        from_coordinate: f64,
        from_value: f64,
        to_coordinate: f64,
        to_value: f64,
    },
    #[serde(rename = "minecraft:noise")]
    Noise {
        noise: ResourceId,
        xz_scale: f64,
        y_scale: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shift_x: Option<Box<DensityFunction>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shift_z: Option<Box<DensityFunction>>,
    },
    #[serde(rename = "minecraft:old_blended_noise")]
    OldBlendedNoise {
        xz_scale: f64,
        y_scale: f64,
        xz_factor: f64,
        y_factor: f64,
        smear_scale_multiplier: f64,
    },
    #[serde(rename = "minecraft:interpolated")]
    Interpolated {
        input: Box<DensityFunction>,
        cell_size_xz: u32,
        cell_size_y: u32,
    },
    #[serde(rename = "minecraft:cache")]
    Cache { input: Box<DensityFunction> },
    #[serde(rename = "minecraft:shift_a")]
    ShiftA { noise: ResourceId },
    #[serde(rename = "minecraft:shift_b")]
    ShiftB { noise: ResourceId },
    #[serde(rename = "minecraft:blend_alpha")]
    BlendAlpha,
    #[serde(rename = "minecraft:blend_offset")]
    BlendOffset,
    #[serde(rename = "minecraft:blend_density")]
    BlendDensity { input: Box<DensityFunction> },
    #[serde(rename = "minecraft:range_choice")]
    RangeChoice {
        input: Box<DensityFunction>,
        min_inclusive: f64,
        max_exclusive: f64,
        when_in_range: Box<DensityFunction>,
        when_out_of_range: Box<DensityFunction>,
    },
    #[serde(rename = "minecraft:interval_select")]
    IntervalSelect {
        input: Box<DensityFunction>,
        thresholds: Vec<f64>,
        functions: Vec<DensityFunction>,
    },
    #[serde(rename = "minecraft:slice")]
    Slice {
        axis: Axis,
        coordinate: i32,
        input: Box<DensityFunction>,
    },
    /// Vanilla's blend/lerp operation with two density-function inputs.
    #[serde(rename = "minecraft:lerp")]
    Lerp {
        alpha: Box<DensityFunction>,
        first: Box<DensityFunction>,
        second: Box<DensityFunction>,
    },
    #[serde(rename = "minecraft:spline")]
    Spline { spline: Box<Spline> },
    #[serde(rename = "minecraft:distance_to_point")]
    DistanceToPoint {
        metric: DistanceMetric,
        point: [f64; 3],
    },
    #[serde(rename = "minecraft:find_top_surface")]
    FindTopSurface {
        density: Box<DensityFunction>,
        upper_bound: Box<DensityFunction>,
        lower_bound: i32,
        cell_height: u32,
    },
    #[serde(rename = "minecraft:end_outer_islands")]
    EndOuterIslands,
    #[serde(rename = "minecraft:beardifier")]
    Beardifier,
}

/// Coordinate axis for gradients and slices.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    X,
    Y,
    Z,
}

/// Distance metric for `minecraft:distance_to_point`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    Euclidean,
    Manhattan,
    Chebyshev,
}

/// A cubic spline: a coordinate function and its sorted control points.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spline {
    pub coordinate: DensityFunction,
    pub points: Vec<SplinePoint>,
}

/// One spline control point. `value` is either a constant or a nested spline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SplinePoint {
    pub location: f64,
    pub derivative: f64,
    pub value: SplineValue,
}

/// The value of a spline control point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SplineValue {
    Constant(f64),
    Spline(Box<Spline>),
}

impl DensityFunction {
    /// Total number of density-function values in the tree.
    ///
    /// Every constant, reference, and operation counts as one. A nested spline object counts
    /// as one node in addition to its coordinate and point values. After
    /// [`Self::resolve_references`] the count is the size of the fully inlined AST.
    pub fn node_count(&self) -> usize {
        let mut nodes = 0;
        let mut references = 0;
        self.accumulate(&mut nodes, &mut references);
        nodes
    }

    /// Number of unresolved named references in the tree.
    pub fn reference_count(&self) -> usize {
        let mut nodes = 0;
        let mut references = 0;
        self.accumulate(&mut nodes, &mut references);
        references
    }

    fn accumulate(&self, nodes: &mut usize, references: &mut usize) {
        *nodes += 1;
        match self {
            Self::Constant(_) => {}
            Self::Reference(_) => *references += 1,
            Self::Node(node) => node.accumulate(nodes, references),
        }
    }
}

impl DensityNode {
    fn accumulate(&self, nodes: &mut usize, references: &mut usize) {
        match self {
            Self::Add { left, right }
            | Self::Sub { left, right }
            | Self::Mul { left, right }
            | Self::Div { left, right }
            | Self::Min { left, right }
            | Self::Max { left, right } => {
                left.accumulate(nodes, references);
                right.accumulate(nodes, references);
            }
            Self::Abs { input }
            | Self::Square { input }
            | Self::Cube { input }
            | Self::HalfNegative { input }
            | Self::QuarterNegative { input }
            | Self::Squeeze { input }
            | Self::Negate { input }
            | Self::Clamp { input, .. }
            | Self::Interpolated { input, .. }
            | Self::Cache { input }
            | Self::BlendDensity { input }
            | Self::Slice { input, .. } => input.accumulate(nodes, references),
            Self::ShiftA { .. } | Self::ShiftB { .. } => {}
            Self::Gradient { .. }
            | Self::OldBlendedNoise { .. }
            | Self::BlendAlpha
            | Self::BlendOffset
            | Self::DistanceToPoint { .. }
            | Self::EndOuterIslands
            | Self::Beardifier => {}
            Self::Noise {
                shift_x, shift_z, ..
            } => {
                if let Some(shift) = shift_x {
                    shift.accumulate(nodes, references);
                }
                if let Some(shift) = shift_z {
                    shift.accumulate(nodes, references);
                }
            }
            Self::RangeChoice {
                input,
                when_in_range,
                when_out_of_range,
                ..
            } => {
                input.accumulate(nodes, references);
                when_in_range.accumulate(nodes, references);
                when_out_of_range.accumulate(nodes, references);
            }
            Self::IntervalSelect {
                input, functions, ..
            } => {
                input.accumulate(nodes, references);
                for function in functions {
                    function.accumulate(nodes, references);
                }
            }
            Self::Lerp {
                alpha,
                first,
                second,
                ..
            } => {
                alpha.accumulate(nodes, references);
                first.accumulate(nodes, references);
                second.accumulate(nodes, references);
            }
            Self::Spline { spline } => spline.accumulate(nodes, references),
            Self::FindTopSurface {
                density,
                upper_bound,
                ..
            } => {
                density.accumulate(nodes, references);
                upper_bound.accumulate(nodes, references);
            }
        }
    }

    pub(crate) fn try_map_children<E>(
        self,
        map: &mut impl FnMut(DensityFunction) -> Result<DensityFunction, E>,
    ) -> Result<Self, E> {
        Ok(match self {
            Self::Add { left, right } => Self::Add {
                left: Box::new(map(*left)?),
                right: Box::new(map(*right)?),
            },
            Self::Sub { left, right } => Self::Sub {
                left: Box::new(map(*left)?),
                right: Box::new(map(*right)?),
            },
            Self::Mul { left, right } => Self::Mul {
                left: Box::new(map(*left)?),
                right: Box::new(map(*right)?),
            },
            Self::Div { left, right } => Self::Div {
                left: Box::new(map(*left)?),
                right: Box::new(map(*right)?),
            },
            Self::Min { left, right } => Self::Min {
                left: Box::new(map(*left)?),
                right: Box::new(map(*right)?),
            },
            Self::Max { left, right } => Self::Max {
                left: Box::new(map(*left)?),
                right: Box::new(map(*right)?),
            },
            Self::Abs { input } => Self::Abs {
                input: Box::new(map(*input)?),
            },
            Self::Square { input } => Self::Square {
                input: Box::new(map(*input)?),
            },
            Self::Cube { input } => Self::Cube {
                input: Box::new(map(*input)?),
            },
            Self::HalfNegative { input } => Self::HalfNegative {
                input: Box::new(map(*input)?),
            },
            Self::QuarterNegative { input } => Self::QuarterNegative {
                input: Box::new(map(*input)?),
            },
            Self::Squeeze { input } => Self::Squeeze {
                input: Box::new(map(*input)?),
            },
            Self::Negate { input } => Self::Negate {
                input: Box::new(map(*input)?),
            },
            Self::Clamp { input, min, max } => Self::Clamp {
                input: Box::new(map(*input)?),
                min,
                max,
            },
            Self::Gradient {
                axis,
                from_coordinate,
                from_value,
                to_coordinate,
                to_value,
            } => Self::Gradient {
                axis,
                from_coordinate,
                from_value,
                to_coordinate,
                to_value,
            },
            Self::Noise {
                noise,
                xz_scale,
                y_scale,
                shift_x,
                shift_z,
            } => Self::Noise {
                noise,
                xz_scale,
                y_scale,
                shift_x: shift_x.map(|shift| map(*shift).map(Box::new)).transpose()?,
                shift_z: shift_z.map(|shift| map(*shift).map(Box::new)).transpose()?,
            },
            Self::OldBlendedNoise {
                xz_scale,
                y_scale,
                xz_factor,
                y_factor,
                smear_scale_multiplier,
            } => Self::OldBlendedNoise {
                xz_scale,
                y_scale,
                xz_factor,
                y_factor,
                smear_scale_multiplier,
            },
            Self::Interpolated {
                input,
                cell_size_xz,
                cell_size_y,
            } => Self::Interpolated {
                input: Box::new(map(*input)?),
                cell_size_xz,
                cell_size_y,
            },
            Self::Cache { input } => Self::Cache {
                input: Box::new(map(*input)?),
            },
            Self::ShiftA { noise } => Self::ShiftA { noise },
            Self::ShiftB { noise } => Self::ShiftB { noise },
            Self::BlendAlpha => Self::BlendAlpha,
            Self::BlendOffset => Self::BlendOffset,
            Self::BlendDensity { input } => Self::BlendDensity {
                input: Box::new(map(*input)?),
            },
            Self::RangeChoice {
                input,
                min_inclusive,
                max_exclusive,
                when_in_range,
                when_out_of_range,
            } => Self::RangeChoice {
                input: Box::new(map(*input)?),
                min_inclusive,
                max_exclusive,
                when_in_range: Box::new(map(*when_in_range)?),
                when_out_of_range: Box::new(map(*when_out_of_range)?),
            },
            Self::IntervalSelect {
                input,
                thresholds,
                functions,
            } => Self::IntervalSelect {
                input: Box::new(map(*input)?),
                thresholds,
                functions: functions
                    .into_iter()
                    .map(&mut *map)
                    .collect::<Result<_, E>>()?,
            },
            Self::Slice {
                axis,
                coordinate,
                input,
            } => Self::Slice {
                axis,
                coordinate,
                input: Box::new(map(*input)?),
            },
            Self::Lerp {
                alpha,
                first,
                second,
            } => Self::Lerp {
                alpha: Box::new(map(*alpha)?),
                first: Box::new(map(*first)?),
                second: Box::new(map(*second)?),
            },
            Self::Spline { spline } => Self::Spline {
                spline: Box::new(spline.try_map(map)?),
            },
            Self::DistanceToPoint { metric, point } => Self::DistanceToPoint { metric, point },
            Self::FindTopSurface {
                density,
                upper_bound,
                lower_bound,
                cell_height,
            } => Self::FindTopSurface {
                density: Box::new(map(*density)?),
                upper_bound: Box::new(map(*upper_bound)?),
                lower_bound,
                cell_height,
            },
            Self::EndOuterIslands => Self::EndOuterIslands,
            Self::Beardifier => Self::Beardifier,
        })
    }
}

impl Spline {
    fn accumulate(&self, nodes: &mut usize, references: &mut usize) {
        self.coordinate.accumulate(nodes, references);
        for point in &self.points {
            match &point.value {
                SplineValue::Constant(_) => *nodes += 1,
                SplineValue::Spline(spline) => {
                    *nodes += 1;
                    spline.accumulate(nodes, references);
                }
            }
        }
    }

    fn try_map<E>(
        self,
        map: &mut impl FnMut(DensityFunction) -> Result<DensityFunction, E>,
    ) -> Result<Self, E> {
        let mut points = Vec::with_capacity(self.points.len());
        for point in self.points {
            let value = match point.value {
                SplineValue::Constant(value) => SplineValue::Constant(value),
                SplineValue::Spline(spline) => SplineValue::Spline(Box::new(spline.try_map(map)?)),
            };
            points.push(SplinePoint {
                location: point.location,
                derivative: point.derivative,
                value,
            });
        }
        Ok(Self {
            coordinate: map(self.coordinate)?,
            points,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DensityFunction, DensityNode, Spline, SplinePoint, SplineValue};
    use crate::id::ResourceId;

    #[test]
    fn round_trips_a_nested_ast() {
        let json = r#"{
            "type": "minecraft:add",
            "left": {
                "type": "minecraft:spline",
                "spline": {
                    "coordinate": "minecraft:overworld/continents",
                    "points": [
                        {"derivative": 0.0, "location": -0.5, "value": 1.5},
                        {
                            "derivative": 0.0,
                            "location": 0.0,
                            "value": {
                                "coordinate": {
                                    "type": "minecraft:noise",
                                    "noise": "minecraft:offset",
                                    "xz_scale": 1.0,
                                    "y_scale": 1.0,
                                    "shift_x": "minecraft:shift_x"
                                },
                                "points": [
                                    {"derivative": 0.25, "location": 1.0, "value": 2.0}
                                ]
                            }
                        }
                    ]
                }
            },
            "right": {
                "type": "minecraft:find_top_surface",
                "cell_height": 8,
                "density": 0.5,
                "lower_bound": -64,
                "upper_bound": {"type": "minecraft:blend_alpha"}
            }
        }"#;
        let parsed: DensityFunction = serde_json::from_str(json).unwrap();
        let serialized = serde_json::to_string(&parsed).unwrap();
        let back: DensityFunction = serde_json::from_str(&serialized).unwrap();
        assert_eq!(back, parsed);
        assert_eq!(back.reference_count(), 2);
        assert_eq!(back.node_count(), 11);
    }

    #[test]
    fn counts_constants_and_nested_splines() {
        let node = DensityFunction::Node(DensityNode::Spline {
            spline: Box::new(Spline {
                coordinate: DensityFunction::Constant(0.0),
                points: vec![
                    SplinePoint {
                        location: 0.0,
                        derivative: 0.0,
                        value: SplineValue::Constant(1.0),
                    },
                    SplinePoint {
                        location: 1.0,
                        derivative: 0.0,
                        value: SplineValue::Spline(Box::new(Spline {
                            coordinate: DensityFunction::Reference(
                                ResourceId::parse("continents").unwrap(),
                            ),
                            points: vec![SplinePoint {
                                location: 0.0,
                                derivative: 0.0,
                                value: SplineValue::Constant(2.0),
                            }],
                        })),
                    },
                ],
            }),
        });
        // spline + coordinate + simple point + nested spline + its coordinate + point
        assert_eq!(node.node_count(), 6);
        assert_eq!(node.reference_count(), 1);
    }
}
