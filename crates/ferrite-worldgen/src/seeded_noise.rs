//! Seeded 26.3 normal-noise samplers.
use crate::{FunctionContext, ResourceId, WorldgenData};
use ferrite_noise::{
    BlendedNoise, Noise, NoiseStack, NormalNoise, PositionalRandomFactory, RandomSource,
    XoroshiroRandomSource,
};
use std::{cell::RefCell, collections::BTreeMap};

type BlendedSampler = Box<dyn Fn(i32, i32, i32) -> f32>;

/// The keyed noises derived from a world seed using vanilla's positional factory.
pub struct SeededNoise {
    noises: BTreeMap<ResourceId, NoiseStack>,
    factory: Box<dyn PositionalRandomFactory>,
    blended: RefCell<BTreeMap<[u64; 5], BlendedSampler>>,
}

impl SeededNoise {
    pub(crate) fn named_random(&self, name: &str) -> Box<dyn RandomSource> {
        self.factory.from_hash_of(name)
    }
    /// The positional factory for a named random, for callers that sample many positions
    /// with one name (surface rules ask for the same random once per block).
    pub(crate) fn positional_factory(&self, name: &ResourceId) -> Box<dyn PositionalRandomFactory> {
        self.factory
            .from_hash_of(&name.to_string())
            .fork_positional()
    }

    pub(crate) fn positional_random(
        &self,
        name: Option<&str>,
        x: i32,
        y: i32,
        z: i32,
    ) -> Box<dyn RandomSource> {
        if let Some(name) = name {
            self.factory
                .from_hash_of(name)
                .fork_positional()
                .at(x, y, z)
        } else {
            self.factory.at(x, y, z)
        }
    }

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
        Self {
            noises,
            factory,
            blended: RefCell::new(BTreeMap::new()),
        }
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
    fn sample_blended(&self, settings: [f64; 5], x: i32, y: i32, z: i32) -> f32 {
        let key = settings.map(f64::to_bits);
        let mut cache = self.noises.blended.borrow_mut();
        let sampler = cache.entry(key).or_insert_with(|| {
            let mut random = self.noises.factory.from_hash_of("minecraft:terrain");
            let noise = BlendedNoise::new(
                settings[0],
                settings[1],
                settings[2],
                settings[3],
                settings[4],
            );
            let sampler = noise.compile_sampler(&mut *random);
            Box::new(move |x, y, z| sampler.sample_value(x, y, z))
        });
        sampler(x, y, z)
    }
}
