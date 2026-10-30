//! Seeded 26.3 normal-noise samplers.
use crate::{FunctionContext, ResourceId, WorldgenData};
use ferrite_noise::{Noise, NoiseStack, NormalNoise, RandomSource, XoroshiroRandomSource};
use std::collections::BTreeMap;

/// The keyed noises derived from a world seed using vanilla's positional factory.
pub struct SeededNoise {
    noises: BTreeMap<ResourceId, NoiseStack>,
}

impl SeededNoise {
    pub fn new(data: &WorldgenData, seed: i64) -> Self {
        let mut source = XoroshiroRandomSource::new(seed);
        let factory = source.fork_positional();
        let noises = data
            .noises
            .iter()
            .map(|(id, parameters)| {
                let mut builder = NormalNoise::builder()
                    .set_base_amplitude(parameters.base_amplitude)
                    .set_base_octave(parameters.base_octave)
                    .set_octave_count(parameters.octaves() as i32);
                if let Some(modifiers) = &parameters.amplitude_modifiers {
                    for (index, value) in modifiers.iter().enumerate() {
                        builder = builder.set_amplitude_modifier(index as i32, *value);
                    }
                }
                let mut random = factory.from_hash_of(&id.to_string());
                (id.clone(), builder.build().create(&mut *random))
            })
            .collect();
        Self { noises }
    }
    pub fn at(&self, x: i32, y: i32, z: i32) -> SeededContext<'_> {
        SeededContext {
            noises: self,
            x,
            y,
            z,
        }
    }
}

/// Block coordinates and seeded noise resources for density evaluation.
pub struct SeededContext<'a> {
    noises: &'a SeededNoise,
    x: i32,
    y: i32,
    z: i32,
}
impl FunctionContext for SeededContext<'_> {
    fn block_x(&self) -> i32 {
        self.x
    }
    fn block_y(&self) -> i32 {
        self.y
    }
    fn block_z(&self) -> i32 {
        self.z
    }
    fn sample_noise(&self, id: &ResourceId, x: f64, y: f64, z: f64) -> f32 {
        self.noises
            .noises
            .get(id)
            .unwrap_or_else(|| panic!("unknown noise {id}"))
            .get(x, y, z)
    }
}
