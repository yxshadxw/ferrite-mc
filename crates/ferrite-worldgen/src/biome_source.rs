//! Multi-noise biome selection.
//!
//! This is a Rust port of the relevant 26.3 vanilla code:
//!
//! * [`quantize_coord`] matches `Climate.quantizeCoord` (`f32 * 10000.0`, truncated),
//! * [`ClimateTargetPoint`] matches `Climate.TargetPoint`,
//! * the per-parameter distance and seven-term fitness match `Climate.Parameter.distance`
//!   and `Climate.ParameterPoint.fitness`,
//! * ties are resolved by the first parameter point in list order, matching
//!   `Climate.ParameterList.findValueBruteForce`.
//!
//! [`MultiNoiseBiomeSource::find_biome`] uses a bulk-loaded R-tree whose bounding boxes
//! prune candidates without changing that tie-breaking rule. The vanilla R-tree additionally
//! keeps a last-result hint and can return a different biome when several parameter points
//! tie with zero fitness (notably the overlapping lush/dripstone cave ranges in the overworld);
//! this implementation deliberately follows the deterministic first-minimum rule.

use thiserror::Error;

use crate::WorldgenData;
use crate::id::ResourceId;
use crate::multi_noise::{MultiNoiseBiomeEntry, MultiNoiseParameters, ParameterRange};

/// Number of climate parameters used by the printer and the tree (six climate axes plus
/// offset).
const PARAMETER_COUNT: usize = 7;

/// Maximum children per interior node. Vanilla uses 19 by default; eight keeps the branch
/// order array on the stack while still giving a shallow tree.
const FANOUT: usize = 8;

/// Converts a climate coordinate to the fixed-point long used by vanilla's climate search.
///
/// Vanilla's `Climate.quantizeCoord` computes `(long) (value * 10000.0F)`, so this uses an
/// `f32` multiply and a truncating cast, not an `f64` multiply.
pub fn quantize_coord(value: f32) -> i64 {
    (value * 10_000.0) as i64
}

/// A six-axis climate target in vanilla fixed-point units (one ten-thousandth per unit).
///
/// This mirrors `Climate.TargetPoint`; the offset axis is a property of the parameter
/// points, not of the target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClimateTargetPoint {
    pub temperature: i64,
    pub humidity: i64,
    pub continentalness: i64,
    pub erosion: i64,
    pub depth: i64,
    pub weirdness: i64,
}

impl ClimateTargetPoint {
    /// Builds a target from already-quantized values.
    pub const fn new(
        temperature: i64,
        humidity: i64,
        continentalness: i64,
        erosion: i64,
        depth: i64,
        weirdness: i64,
    ) -> Self {
        Self {
            temperature,
            humidity,
            continentalness,
            erosion,
            depth,
            weirdness,
        }
    }

    /// Quantizes six float coordinates exactly like `Climate.target`.
    pub fn quantized(
        temperature: f32,
        humidity: f32,
        continentalness: f32,
        erosion: f32,
        depth: f32,
        weirdness: f32,
    ) -> Self {
        Self::new(
            quantize_coord(temperature),
            quantize_coord(humidity),
            quantize_coord(continentalness),
            quantize_coord(erosion),
            quantize_coord(depth),
            quantize_coord(weirdness),
        )
    }

    fn as_array(self) -> [i64; 6] {
        [
            self.temperature,
            self.humidity,
            self.continentalness,
            self.erosion,
            self.depth,
            self.weirdness,
        ]
    }
}

/// Failure while constructing a multi-noise biome source.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum MultiNoiseError {
    #[error("unknown multi-noise preset `{preset}`")]
    UnknownPreset { preset: String },
    #[error("no generated biome parameter list for `{parameters}`")]
    MissingParameters { parameters: ResourceId },
    #[error("the multi-noise parameter list is empty")]
    Empty,
    #[error("biome `{biome}` has a range offset; vanilla requires a scalar offset")]
    OffsetRange { biome: ResourceId },
}

/// A multi-noise biome source: an ordered parameter list plus its search index.
///
/// The list order is the order in `reports/biome_parameters/<preset>.json` (which matches
/// vanilla's preset order). Ties in fitness are resolved by that order.
#[derive(Clone, Debug)]
pub struct MultiNoiseBiomeSource {
    entries: Vec<Entry>,
    root: Node,
}

impl MultiNoiseBiomeSource {
    /// Builds a source from a generated parameter list.
    pub fn from_parameters(parameters: &MultiNoiseParameters) -> Result<Self, MultiNoiseError> {
        let mut entries = Vec::with_capacity(parameters.biomes.len());
        for entry in &parameters.biomes {
            entries.push(Entry {
                biome: entry.biome.clone(),
                point: QuantizedPoint::from_entry(entry)?,
            });
        }
        if entries.is_empty() {
            return Err(MultiNoiseError::Empty);
        }

        let mut indices: Vec<usize> = (0..entries.len()).collect();
        let root = build(&entries, &mut indices);
        Ok(Self { entries, root })
    }

    /// Loads a preset (`minecraft:overworld` or `minecraft:nether`) from already-loaded
    /// worldgen data.
    ///
    /// The preset entry under `worldgen/multi_noise_biome_source_parameter_list` names the
    /// generated list under `reports/biome_parameters`.
    pub fn from_worldgen(data: &WorldgenData, preset: &str) -> Result<Self, MultiNoiseError> {
        let preset_entry =
            data.multi_noise_preset(preset)
                .ok_or_else(|| MultiNoiseError::UnknownPreset {
                    preset: preset.to_owned(),
                })?;
        let preset_name = preset_entry.preset.to_string();
        let parameters = data.biome_parameters(&preset_name).ok_or_else(|| {
            MultiNoiseError::MissingParameters {
                parameters: preset_entry.preset.clone(),
            }
        })?;
        Self::from_parameters(parameters)
    }

    /// Number of parameter points (biomes may appear more than once).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the source has no parameter points. Construction rejects empty lists.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Biomes in parameter-list order, including duplicates.
    pub fn biomes(&self) -> impl Iterator<Item = &ResourceId> {
        self.entries.iter().map(|entry| &entry.biome)
    }

    /// Finds the biome with minimum fitness using the R-tree index.
    ///
    /// Ties are resolved by the first matching parameter point in list order.
    pub fn find_biome(&self, target: ClimateTargetPoint) -> &ResourceId {
        let index = self.find_index(&target.as_array());
        &self.entries[index].biome
    }

    /// Reference implementation that scans the whole parameter list in order.
    pub fn find_biome_brute_force(&self, target: ClimateTargetPoint) -> &ResourceId {
        let index = self.find_index_brute_force(&target.as_array());
        &self.entries[index].biome
    }

    /// Convenience wrapper around [`Self::find_biome`] for six already-quantized values.
    pub fn find_biome_at(
        &self,
        temperature: i64,
        humidity: i64,
        continentalness: i64,
        erosion: i64,
        depth: i64,
        weirdness: i64,
    ) -> &ResourceId {
        self.find_biome(ClimateTargetPoint::new(
            temperature,
            humidity,
            continentalness,
            erosion,
            depth,
            weirdness,
        ))
    }

    /// Convenience wrapper that quantizes six float values like `Climate.target`.
    pub fn find_biome_at_values(
        &self,
        temperature: f32,
        humidity: f32,
        continentalness: f32,
        erosion: f32,
        depth: f32,
        weirdness: f32,
    ) -> &ResourceId {
        self.find_biome(ClimateTargetPoint::quantized(
            temperature,
            humidity,
            continentalness,
            erosion,
            depth,
            weirdness,
        ))
    }

    fn find_index(&self, target: &[i64; 6]) -> usize {
        let mut best = None;
        let root_bound = self.root.bounds.distance(target);
        self.search(&self.root, target, root_bound, &mut best);
        best.expect("a non-empty source always has a candidate").1
    }

    fn find_index_brute_force(&self, target: &[i64; 6]) -> usize {
        let mut best: Option<(i64, usize)> = None;
        for (index, entry) in self.entries.iter().enumerate() {
            let fitness = entry.point.fitness(target);
            if fitness == 0 {
                // No later point can beat zero fitness; the first zero is the minimum.
                return index;
            }
            let better = match best {
                None => true,
                Some((best_fitness, _)) => fitness < best_fitness,
            };
            if better {
                best = Some((fitness, index));
            }
        }
        best.expect("a non-empty source always has a candidate").1
    }

    fn search(&self, node: &Node, target: &[i64; 6], bound: i64, best: &mut Option<(i64, usize)>) {
        if let Some((best_fitness, best_index)) = *best {
            if bound > best_fitness {
                return;
            }
            if bound == best_fitness && node.min_index >= best_index {
                return;
            }
        }

        match &node.kind {
            NodeKind::Leaf(index) => {
                let fitness = self.entries[*index].point.fitness(target);
                let better = match *best {
                    None => true,
                    Some((best_fitness, best_index)) => {
                        fitness < best_fitness || (fitness == best_fitness && *index < best_index)
                    }
                };
                if better {
                    *best = Some((fitness, *index));
                }
            }
            NodeKind::Branch(children) => {
                debug_assert!(children.len() <= FANOUT);
                let mut order = [0usize; FANOUT];
                let mut keys = [(i64::MAX, usize::MAX); FANOUT];
                for (position, child) in children.iter().enumerate() {
                    order[position] = position;
                    keys[position] = (child.bounds.distance(target), child.min_index);
                }

                // Search the most promising child first so the best fitness improves early.
                for position in 1..children.len() {
                    let mut current = position;
                    while current > 0 && keys[order[current - 1]] > keys[order[current]] {
                        order.swap(current - 1, current);
                        current -= 1;
                    }
                }

                for &position in order.iter().take(children.len()) {
                    let child = &children[position];
                    let child_bound = keys[position].0;
                    if let Some((best_fitness, best_index)) = *best
                        && (child_bound > best_fitness
                            || (child_bound == best_fitness && child.min_index >= best_index))
                    {
                        // Children are sorted by (bound, min index), so everything left can
                        // only be equal or worse.
                        break;
                    }
                    self.search(child, target, child_bound, best);
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Entry {
    biome: ResourceId,
    point: QuantizedPoint,
}

/// A parameter range in vanilla fixed-point units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Parameter {
    min: i64,
    max: i64,
}

impl Parameter {
    const fn point(value: i64) -> Self {
        Self {
            min: value,
            max: value,
        }
    }

    fn from_range(range: ParameterRange) -> Self {
        match range {
            ParameterRange::Value(value) => Self::point(quantize_coord(value as f32)),
            ParameterRange::Range([min, max]) => Self {
                min: quantize_coord(min as f32),
                max: quantize_coord(max as f32),
            },
        }
    }

    /// `Climate.Parameter.distance`: zero inside the range, otherwise the distance to the
    /// nearer end. The arithmetic intentionally wraps like Java's `long`.
    fn distance(self, value: i64) -> i64 {
        let above_max = value.wrapping_sub(self.max);
        if above_max > 0 {
            above_max
        } else {
            self.min.wrapping_sub(value).max(0)
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    fn span(self) -> i64 {
        self.max - self.min
    }

    fn center(self) -> i64 {
        self.min + (self.max - self.min) / 2
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Bounds([Parameter; PARAMETER_COUNT]);

impl Bounds {
    fn from_point(point: QuantizedPoint) -> Self {
        Self(point.parameters)
    }

    fn union(self, other: Self) -> Self {
        let mut parameters = self.0;
        for (parameter, other) in parameters.iter_mut().zip(other.0) {
            *parameter = parameter.union(other);
        }
        Self(parameters)
    }

    /// A lower bound on the fitness of every point inside these bounds.
    fn distance(self, target: &[i64; 6]) -> i64 {
        let mut sum = 0i64;
        for (parameter, value) in self.0[..6].iter().zip(target) {
            let distance = parameter.distance(*value);
            sum = sum.wrapping_add(distance.wrapping_mul(distance));
        }
        let offset = self.0[6].min;
        sum.wrapping_add(offset.wrapping_mul(offset))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct QuantizedPoint {
    /// Temperature, humidity, continentalness, erosion, depth, weirdness, offset.
    parameters: [Parameter; PARAMETER_COUNT],
}

impl QuantizedPoint {
    fn from_entry(entry: &MultiNoiseBiomeEntry) -> Result<Self, MultiNoiseError> {
        let parameters = &entry.parameters;
        let offset = match parameters.offset {
            ParameterRange::Value(value) => quantize_coord(value as f32),
            ParameterRange::Range(_) => {
                return Err(MultiNoiseError::OffsetRange {
                    biome: entry.biome.clone(),
                });
            }
        };
        Ok(Self {
            parameters: [
                Parameter::from_range(parameters.temperature),
                Parameter::from_range(parameters.humidity),
                Parameter::from_range(parameters.continentalness),
                Parameter::from_range(parameters.erosion),
                Parameter::from_range(parameters.depth),
                Parameter::from_range(parameters.weirdness),
                Parameter::point(offset),
            ],
        })
    }

    /// `Climate.ParameterPoint.fitness`: squared range distance on six axes plus squared
    /// offset. The arithmetic intentionally wraps like Java's `long`.
    fn fitness(self, target: &[i64; 6]) -> i64 {
        let mut sum = 0i64;
        for (parameter, value) in self.parameters[..6].iter().zip(target) {
            let distance = parameter.distance(*value);
            sum = sum.wrapping_add(distance.wrapping_mul(distance));
        }
        let offset = self.parameters[6].min;
        sum.wrapping_add(offset.wrapping_mul(offset))
    }

    fn bounds(self) -> Bounds {
        Bounds::from_point(self)
    }
}

#[derive(Clone, Debug)]
struct Node {
    bounds: Bounds,
    min_index: usize,
    kind: NodeKind,
}

#[derive(Clone, Debug)]
enum NodeKind {
    Leaf(usize),
    Branch(Vec<Node>),
}

impl Node {
    fn leaf(index: usize, point: QuantizedPoint) -> Self {
        Self {
            bounds: point.bounds(),
            min_index: index,
            kind: NodeKind::Leaf(index),
        }
    }

    fn branch(children: Vec<Node>) -> Self {
        assert!(!children.is_empty(), "a branch needs at least one child");
        let mut children_iter = children.iter();
        let first = children_iter.next().expect("checked non-empty");
        let mut bounds = first.bounds;
        let mut min_index = first.min_index;
        for child in children_iter {
            bounds = bounds.union(child.bounds);
            min_index = min_index.min(child.min_index);
        }
        Self {
            bounds,
            min_index,
            kind: NodeKind::Branch(children),
        }
    }
}

/// Bulk-loads the R-tree by recursively splitting the entries along the widest axis.
fn build(entries: &[Entry], indices: &mut [usize]) -> Node {
    debug_assert!(!indices.is_empty());
    if indices.len() <= FANOUT {
        let children = indices
            .iter()
            .map(|&index| Node::leaf(index, entries[index].point))
            .collect();
        return Node::branch(children);
    }

    let mut bounds = entries[indices[0]].point.bounds();
    for &index in &indices[1..] {
        bounds = bounds.union(entries[index].point.bounds());
    }
    let widest = (0..PARAMETER_COUNT)
        .max_by_key(|&axis| bounds.0[axis].span())
        .expect("there is at least one parameter");
    indices
        .sort_unstable_by_key(|&index| (entries[index].point.parameters[widest].center(), index));

    let chunk_size = indices.len().div_ceil(FANOUT);
    let mut children = Vec::with_capacity(FANOUT);
    for chunk in indices.chunks_mut(chunk_size) {
        children.push(build(entries, chunk));
    }
    Node::branch(children)
}

#[cfg(test)]
mod tests {
    use super::{
        ClimateTargetPoint, MultiNoiseBiomeSource, MultiNoiseError, ParameterRange, quantize_coord,
    };
    use crate::id::ResourceId;
    use crate::multi_noise::{BiomeParameterPoint, MultiNoiseBiomeEntry, MultiNoiseParameters};

    fn entry(biome: &str) -> MultiNoiseBiomeEntry {
        let point = || ParameterRange::Value(0.0);
        MultiNoiseBiomeEntry {
            biome: ResourceId::parse(biome).unwrap(),
            parameters: BiomeParameterPoint {
                continentalness: point(),
                depth: point(),
                erosion: point(),
                humidity: point(),
                offset: point(),
                temperature: point(),
                weirdness: point(),
            },
        }
    }

    #[test]
    fn quantization_matches_vanilla_float_math() {
        assert_eq!(quantize_coord(1.0), 10_000);
        assert_eq!(quantize_coord(0.9), 9_000);
        assert_eq!(quantize_coord(-1.2), -12_000);
        assert_eq!(quantize_coord(-1.05), -10_500);
        assert_eq!(quantize_coord(0.375), 3_750);
        assert_eq!(quantize_coord(-0.455), -4_550);
    }

    #[test]
    fn ties_choose_the_first_parameter_point() {
        let parameters = MultiNoiseParameters {
            biomes: vec![entry("minecraft:first"), entry("minecraft:second")],
        };
        let source = MultiNoiseBiomeSource::from_parameters(&parameters).unwrap();
        let target = ClimateTargetPoint::new(0, 0, 0, 0, 0, 0);
        assert_eq!(source.find_biome(target).to_string(), "minecraft:first");
        assert_eq!(
            source.find_biome_brute_force(target).to_string(),
            "minecraft:first"
        );
    }

    #[test]
    fn offset_contributes_squared_distance() {
        let mut far = entry("minecraft:far_offset");
        far.parameters.offset = ParameterRange::Value(0.375);
        let parameters = MultiNoiseParameters {
            biomes: vec![far, entry("minecraft:zero_offset")],
        };
        let source = MultiNoiseBiomeSource::from_parameters(&parameters).unwrap();
        let target = ClimateTargetPoint::new(0, 0, 0, 0, 0, 0);
        assert_eq!(
            source.find_biome(target).to_string(),
            "minecraft:zero_offset"
        );
        assert_eq!(
            source.find_biome_brute_force(target).to_string(),
            "minecraft:zero_offset"
        );
    }

    #[test]
    fn empty_lists_and_range_offsets_are_rejected() {
        assert_eq!(
            MultiNoiseBiomeSource::from_parameters(&MultiNoiseParameters { biomes: vec![] })
                .unwrap_err(),
            MultiNoiseError::Empty
        );

        let mut invalid = entry("minecraft:bad_offset");
        invalid.parameters.offset = ParameterRange::Range([0.0, 1.0]);
        let error = MultiNoiseBiomeSource::from_parameters(&MultiNoiseParameters {
            biomes: vec![invalid],
        })
        .unwrap_err();
        assert!(matches!(error, MultiNoiseError::OffsetRange { .. }));
    }
}
