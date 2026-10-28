//! `BlendedNoise`, the terrain noise blender used by the vanilla noise router.

use super::{Noise, NoiseStack, SmearedPerlinNoise};
use crate::random::RandomSource;

const BASE_SCALE: f64 = 684.412;
const LIMIT_FACTOR: f64 = 0.999_984_741_210_937_5;
const MAIN_FACTOR: f64 = 12.75;
const LIMIT_FIRST_OCTAVE: i32 = -15;
const MAIN_FIRST_OCTAVE: i32 = -7;

/// `BlendedNoise.FbmSet`: the min-limit, max-limit and main noise stacks.
pub struct FbmSet {
    pub min_limit_noise: NoiseStack,
    pub max_limit_noise: NoiseStack,
    pub main_noise: NoiseStack,
}

/// `net.minecraft.world.level.levelgen.synth.BlendedNoise`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendedNoise {
    pub xz_scale: f64,
    pub y_scale: f64,
    pub xz_factor: f64,
    pub y_factor: f64,
    pub smear_scale_multiplier: f64,
}

impl BlendedNoise {
    pub const fn new(
        xz_scale: f64,
        y_scale: f64,
        xz_factor: f64,
        y_factor: f64,
        smear_scale_multiplier: f64,
    ) -> Self {
        Self {
            xz_scale,
            y_scale,
            xz_factor,
            y_factor,
            smear_scale_multiplier,
        }
    }

    pub fn xz_multiplier(&self) -> f64 {
        BASE_SCALE * self.xz_scale
    }

    pub fn y_multiplier(&self) -> f64 {
        BASE_SCALE * self.y_scale
    }

    /// `BlendedNoise.createFbmSet(RandomSource)`.
    pub fn create_fbm_set(&self, random: &mut dyn RandomSource) -> FbmSet {
        let smear_scale = self.y_multiplier() * self.smear_scale_multiplier;
        let main_y_scale = smear_scale / self.y_factor;
        FbmSet {
            min_limit_noise: Self::create_fbm(
                random,
                LIMIT_FIRST_OCTAVE,
                smear_scale,
                LIMIT_FACTOR,
            ),
            max_limit_noise: Self::create_fbm(
                random,
                LIMIT_FIRST_OCTAVE,
                smear_scale,
                LIMIT_FACTOR,
            ),
            main_noise: Self::create_fbm(random, MAIN_FIRST_OCTAVE, main_y_scale, MAIN_FACTOR),
        }
    }

    /// `BlendedNoise.createFbm(RandomSource, int, double, double)`.
    pub fn create_fbm(
        random: &mut dyn RandomSource,
        first_octave: i32,
        y_scale: f64,
        mut amplitude: f64,
    ) -> NoiseStack {
        assert!(first_octave <= 0, "firstOctave>0");
        let octave_count = -first_octave + 1;
        let mut frequency = 1.0f64;
        amplitude /= 2f64.powi(octave_count) - 1.0;

        let mut builder = NoiseStack::builder();
        let mut index = octave_count - 1;
        while index >= 0 {
            builder = builder.add(
                SmearedPerlinNoise::new(random, y_scale * frequency),
                frequency,
                amplitude as f32,
            );
            frequency /= 2.0;
            amplitude *= 2.0;
            index -= 1;
        }
        builder.build()
    }

    /// `BlendedNoise.compileSampler(RandomSource)`.
    pub fn compile_sampler(&self, random: &mut dyn RandomSource) -> BlendedNoiseSampler {
        let fbm_set = self.create_fbm_set(random);
        let xz_multiplier = self.xz_multiplier();
        let y_multiplier = self.y_multiplier();
        BlendedNoiseSampler {
            min_limit_noise: fbm_set.min_limit_noise,
            max_limit_noise: fbm_set.max_limit_noise,
            main_noise: fbm_set.main_noise,
            xz_multiplier,
            y_multiplier,
            main_xz_scale: xz_multiplier / self.xz_factor,
            main_y_scale: y_multiplier / self.y_factor,
        }
    }
}

/// The sampler produced by `BlendedNoise.compileSampler`.
pub struct BlendedNoiseSampler {
    min_limit_noise: NoiseStack,
    max_limit_noise: NoiseStack,
    main_noise: NoiseStack,
    xz_multiplier: f64,
    y_multiplier: f64,
    main_xz_scale: f64,
    main_y_scale: f64,
}

impl BlendedNoiseSampler {
    /// `DensitySampler.sampleValue` for the compiled blended-noise tree.
    pub fn sample_value(&self, x: i32, y: i32, z: i32) -> f32 {
        let x = f64::from(x);
        let y = f64::from(y);
        let z = f64::from(z);

        let min = self.min_limit_noise.get(
            x * self.xz_multiplier,
            y * self.y_multiplier,
            z * self.xz_multiplier,
        );
        let max = self.max_limit_noise.get(
            x * self.xz_multiplier,
            y * self.y_multiplier,
            z * self.xz_multiplier,
        );
        let main = self.main_noise.get(
            x * self.main_xz_scale,
            y * self.main_y_scale,
            z * self.main_xz_scale,
        );

        let alpha = clamp(main + 0.5, 0.0, 1.0);
        if alpha == 0.0 {
            min
        } else if alpha == 1.0 {
            max
        } else {
            min + alpha * (max - min)
        }
    }
}

/// `Mth.clamp(float, float, float)`.
fn clamp(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value.is_nan() {
        f32::NAN
    } else {
        value.min(max)
    }
}
