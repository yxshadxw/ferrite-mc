//! `NoiseStack` (the 26.3 octave container) and `LegacyFbmInitializer`.

use super::{Noise, PerlinNoise};
use crate::random::RandomSource;

/// One octave of a [`NoiseStack`], mirroring `NoiseStack.Layer`.
pub struct NoiseStackLayer {
    pub noise: Box<dyn Noise>,
    pub frequency: f64,
    pub amplitude: f32,
}

/// `net.minecraft.world.level.levelgen.synth.NoiseStack`: a weighted sum of
/// noises at different frequencies.
pub struct NoiseStack {
    layers: Vec<NoiseStackLayer>,
}

impl NoiseStack {
    pub fn builder() -> NoiseStackBuilder {
        NoiseStackBuilder::default()
    }

    pub fn layers(&self) -> &[NoiseStackLayer] {
        &self.layers
    }

    /// `NoiseStack.getLayer(int)`.
    pub fn get_layer(&self, index: usize) -> &dyn Noise {
        &*self.layers[index].noise
    }

    /// `LegacyFbmInitializer.createForLegacyNetherBiome(RandomSource, int, DoubleList)`.
    ///
    /// Kept as an associated function on `NoiseStack` because that is what it
    /// returns; it is used by `NormalNoise.createForLegacyNetherBiome`.
    pub fn create_for_legacy_nether_biome(
        random: &mut dyn RandomSource,
        base_octave: i32,
        amplitudes: &[f64],
    ) -> Self {
        let length = amplitudes.len();
        let index = -base_octave;
        let mut noises: Vec<Option<PerlinNoise>> = (0..length).map(|_| None).collect();

        let first = PerlinNoise::new(random);
        if index >= 0 && index < length as i32 && amplitudes[index as usize] != 0.0 {
            noises[index as usize] = Some(first);
        }

        let mut current = index - 1;
        while current >= 0 {
            if current < length as i32 {
                if amplitudes[current as usize] != 0.0 {
                    noises[current as usize] = Some(PerlinNoise::new(random));
                } else {
                    random.consume_count(262);
                }
            } else {
                random.consume_count(262);
            }
            current -= 1;
        }

        let expected = amplitudes.iter().filter(|value| **value != 0.0).count();
        let created = noises.iter().filter(|noise| noise.is_some()).count();
        assert_eq!(
            created, expected,
            "Failed to create correct number of noise levels for given non-zero amplitudes"
        );
        assert!(
            index >= length as i32 - 1,
            "Positive octaves are temporarily disabled"
        );

        let mut frequency = 2f64.powi(-index);
        let mut amplitude = 2f64.powi(length as i32 - 1) / (2f64.powi(length as i32) - 1.0);
        let mut builder = Self::builder();
        for (position, noise) in noises.into_iter().enumerate() {
            if let Some(noise) = noise {
                builder = builder.add(noise, frequency, (amplitude * amplitudes[position]) as f32);
            }
            frequency *= 2.0;
            amplitude /= 2.0;
        }
        builder.build()
    }
}

impl Noise for NoiseStack {
    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let mut value = 0.0f32;
        for layer in &self.layers {
            let frequency = layer.frequency;
            value += layer.amplitude * layer.noise.get(x * frequency, y * frequency, z * frequency);
        }
        value
    }

    fn get_2d(&self, x: f64, z: f64) -> f32 {
        let mut value = 0.0f32;
        for layer in &self.layers {
            let frequency = layer.frequency;
            value += layer.amplitude * layer.noise.get_2d(x * frequency, z * frequency);
        }
        value
    }
}

/// `NoiseStack.Builder`.
#[derive(Default)]
pub struct NoiseStackBuilder {
    layers: Vec<NoiseStackLayer>,
}

impl NoiseStackBuilder {
    pub fn add<N: Noise + 'static>(mut self, noise: N, frequency: f64, amplitude: f32) -> Self {
        self.layers.push(NoiseStackLayer {
            noise: Box::new(noise),
            frequency,
            amplitude,
        });
        self
    }

    /// `Builder.addStack`: re-parents each layer at a scaled frequency and
    /// amplitude. Takes ownership because Rust noise objects are not shared.
    pub fn add_stack(mut self, stack: NoiseStack, frequency: f64, amplitude: f32) -> Self {
        for layer in stack.layers {
            self.layers.push(NoiseStackLayer {
                noise: layer.noise,
                frequency: layer.frequency * frequency,
                amplitude: layer.amplitude * amplitude,
            });
        }
        self
    }

    pub fn build(self) -> NoiseStack {
        NoiseStack {
            layers: self.layers,
        }
    }
}
