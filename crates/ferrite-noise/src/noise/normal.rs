//! `NormalNoise`: the octave-stacked noise used by vanilla noise routers.

use super::{NoiseStack, PerlinNoise};
use crate::random::RandomSource;

/// Standard deviation of a single vanilla Perlin octave.
const STANDARD_DEVIATION: f64 = 0.270_224_783_124_521_1;
const INPUT_FACTOR: f64 = 1.018_126_888_217_522_7;
const TARGET_DEVIATION: f64 = 1.0 / 3.0;

/// `NormalNoise.Normalization`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Normalization {
    Disabled,
    Enabled,
    Legacy,
}

/// `NormalNoise.Parameters`.
#[derive(Clone, Debug, PartialEq)]
pub struct Parameters {
    pub base_amplitude: f64,
    pub base_octave: i32,
    pub octave_count: i32,
    pub normalize: Normalization,
    pub amplitude_modifiers: Vec<f64>,
}

/// One `NormalNoise.OctaveInfo` entry.
#[derive(Clone, Debug, PartialEq)]
pub struct OctaveInfo {
    pub octave_index: i32,
    pub frequency: f64,
    pub amplitude: f64,
}

impl OctaveInfo {
    /// `OctaveInfo.seed()`: the string hashed by the positional factories.
    pub fn seed(&self) -> String {
        format!("octave_{}", self.octave_index)
    }

    pub fn abs_amplitude(&self) -> f64 {
        self.amplitude.abs()
    }
}

/// `net.minecraft.world.level.levelgen.synth.NormalNoise`.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalNoise {
    parameters: Parameters,
    octaves: Vec<OctaveInfo>,
    normalization_factor: f64,
}

impl NormalNoise {
    fn new(parameters: Parameters) -> Self {
        let normalize = parameters.normalize != Normalization::Disabled;
        let octaves = Self::build_octaves(
            parameters.base_octave,
            parameters.base_amplitude,
            parameters.octave_count,
            normalize,
            &parameters.amplitude_modifiers,
        );

        let mut amplitude_sum: f64 = octaves.iter().map(OctaveInfo::abs_amplitude).sum();
        let mut normalization_factor = Self::compute_normalization_factor(amplitude_sum, &octaves);

        if parameters.normalize == Normalization::Legacy && normalization_factor != 0.0 {
            let parity_factor = Self::compute_parity_normalization_factor(
                parameters.base_amplitude,
                parameters.octave_count,
                &parameters.amplitude_modifiers,
            );
            amplitude_sum = amplitude_sum * parity_factor / normalization_factor;
            normalization_factor = parity_factor;
        }

        // `amplitude_sum` only feeds the range, which this port does not model;
        // keep it out of the struct while still performing vanilla's arithmetic.
        let _ = amplitude_sum;

        Self {
            parameters,
            octaves,
            normalization_factor,
        }
    }

    fn build_octaves(
        base_octave: i32,
        base_amplitude: f64,
        octave_count: i32,
        normalize: bool,
        amplitude_modifiers: &[f64],
    ) -> Vec<OctaveInfo> {
        let mut frequency = 2f64.powi(base_octave);
        let mut amplitude = base_amplitude;
        if normalize {
            amplitude *= 0.5f64.powi(-(octave_count - 1)) / (0.5f64.powi(-octave_count) - 1.0);
        }

        let mut octaves = Vec::with_capacity(octave_count.max(0) as usize);
        for index in 0..octave_count {
            let modifier = Self::amplitude_modifier(amplitude_modifiers, index);
            if modifier != 0.0 {
                octaves.push(OctaveInfo {
                    octave_index: base_octave + index,
                    frequency,
                    amplitude: amplitude * modifier,
                });
            }
            frequency *= 2.0;
            amplitude *= 0.5;
        }
        octaves
    }

    fn amplitude_modifier(amplitude_modifiers: &[f64], index: i32) -> f64 {
        if amplitude_modifiers.is_empty() {
            1.0
        } else {
            amplitude_modifiers[index as usize]
        }
    }

    fn estimate_deviation(octaves: &[OctaveInfo]) -> f64 {
        let mut sum = 0.0;
        for octave in octaves {
            let deviation = STANDARD_DEVIATION * octave.abs_amplitude();
            sum += deviation * deviation;
        }
        sum.sqrt()
    }

    fn compute_normalization_factor(amplitude_sum: f64, octaves: &[OctaveInfo]) -> f64 {
        let deviation = Self::estimate_deviation(octaves);
        if deviation == 0.0 {
            return 0.0;
        }
        let scaled_deviation = deviation * 2f64.sqrt();
        let target = amplitude_sum * TARGET_DEVIATION;
        target / scaled_deviation
    }

    fn compute_parity_normalization_factor(
        base_amplitude: f64,
        octave_count: i32,
        amplitude_modifiers: &[f64],
    ) -> f64 {
        let mut first = i32::MAX;
        let mut last = i32::MIN;
        for index in 0..octave_count {
            if Self::amplitude_modifier(amplitude_modifiers, index) != 0.0 {
                first = first.min(index);
                last = last.max(index);
            }
        }
        let span = last.wrapping_sub(first);
        // `parityExpectedDeviation`: 0.1 * (1 + 1 / (span + 1)).
        let expected = 0.1 * (1.0 + 1.0 / f64::from(span.wrapping_add(1)));
        base_amplitude * 0.5 * TARGET_DEVIATION / expected
    }

    /// `NormalNoise.create(RandomSource)`: builds the two-Perlin-per-octave
    /// noise stack.
    pub fn create(&self, random: &mut dyn RandomSource) -> NoiseStack {
        let first_factory = random.fork_positional();
        let second_factory = random.fork_positional();
        let mut builder = NoiseStack::builder();

        for octave in &self.octaves {
            let seed = octave.seed();
            let mut first_seed = first_factory.from_hash_of(&seed);
            let mut second_seed = second_factory.from_hash_of(&seed);
            let first = PerlinNoise::new(&mut *first_seed);
            let second = PerlinNoise::new(&mut *second_seed);
            let amplitude = (self.normalization_factor * octave.amplitude) as f32;

            builder = builder.add(first, octave.frequency, amplitude);
            builder = builder.add(second, octave.frequency * INPUT_FACTOR, amplitude);
        }

        builder.build()
    }

    /// `NormalNoise.createForLegacyNetherBiome(RandomSource)`.
    pub fn create_for_legacy_nether_biome(&self, random: &mut dyn RandomSource) -> NoiseStack {
        let mut modifiers = self.parameters.amplitude_modifiers.clone();
        if modifiers.is_empty() {
            modifiers = vec![1.0; self.parameters.octave_count.max(0) as usize];
        }

        let first = NoiseStack::create_for_legacy_nether_biome(
            random,
            self.parameters.base_octave,
            &modifiers,
        );
        let second = NoiseStack::create_for_legacy_nether_biome(
            random,
            self.parameters.base_octave,
            &modifiers,
        );
        let amplitude = (self.normalization_factor * self.parameters.base_amplitude) as f32;

        NoiseStack::builder()
            .add_stack(first, 1.0, amplitude)
            .add_stack(second, INPUT_FACTOR, amplitude)
            .build()
    }

    pub fn parameters(&self) -> &Parameters {
        &self.parameters
    }

    pub fn octaves(&self) -> &[OctaveInfo] {
        &self.octaves
    }

    pub fn normalization_factor(&self) -> f64 {
        self.normalization_factor
    }

    /// `NormalNoise.builder()`.
    pub fn builder() -> NormalNoiseBuilder {
        NormalNoiseBuilder::default()
    }

    /// Constructs directly from parameters, bypassing the builder. Mirrors the
    /// private `NormalNoise(Parameters)` constructor.
    pub fn from_parameters(parameters: Parameters) -> Self {
        Self::new(parameters)
    }
}

/// `NormalNoise.Builder`.
#[derive(Clone, Debug)]
pub struct NormalNoiseBuilder {
    base_amplitude: f64,
    base_octave: i32,
    octave_count: i32,
    normalize: Normalization,
    amplitude_modifiers: Vec<f64>,
}

impl Default for NormalNoiseBuilder {
    fn default() -> Self {
        Self {
            base_amplitude: 1.0,
            base_octave: 0,
            octave_count: 1,
            normalize: Normalization::Enabled,
            amplitude_modifiers: Vec::new(),
        }
    }
}

impl NormalNoiseBuilder {
    pub fn set_base_amplitude(mut self, value: f64) -> Self {
        self.base_amplitude = value;
        self
    }

    pub fn set_base_octave(mut self, value: i32) -> Self {
        self.base_octave = value;
        self
    }

    pub fn set_octave_count(mut self, value: i32) -> Self {
        assert!(
            self.amplitude_modifiers.is_empty(),
            "Cannot set octave count after setting amplitude modifier"
        );
        self.octave_count = value;
        self
    }

    pub fn set_normalize(mut self, normalize: bool) -> Self {
        self.normalize = if normalize {
            Normalization::Enabled
        } else {
            Normalization::Disabled
        };
        self
    }

    pub fn set_legacy_normalization(mut self) -> Self {
        self.normalize = Normalization::Legacy;
        self
    }

    pub fn set_amplitude_modifier(mut self, index: i32, value: f64) -> Self {
        assert!(
            index >= 0 && index < self.octave_count,
            "octave index {index} out of bounds for count {}",
            self.octave_count
        );
        if self.amplitude_modifiers.is_empty() {
            self.amplitude_modifiers = vec![1.0; self.octave_count.max(0) as usize];
        }
        self.amplitude_modifiers[index as usize] = value;
        self
    }

    pub fn build(self) -> NormalNoise {
        NormalNoise::new(Parameters {
            base_amplitude: self.base_amplitude,
            base_octave: self.base_octave,
            octave_count: self.octave_count,
            normalize: self.normalize,
            amplitude_modifiers: self.amplitude_modifiers,
        })
    }
}
