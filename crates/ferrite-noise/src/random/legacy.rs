//! `net.minecraft.world.level.levelgen.LegacyRandomSource` and
//! `LegacyRandomSource.LegacyPositionalRandomFactory`.

use super::{MarsagliaPolarGaussian, PositionalRandomFactory, RandomSource};

const MODULUS_BITS: i32 = 48;
const MODULUS_MASK: i64 = (1 << MODULUS_BITS) - 1;
const MULTIPLIER: i64 = 25_214_903_917;
const INCREMENT: i64 = 11;
const DOUBLE_MULTIPLIER: f64 = 1.110_223_024_625_156_5E-16;
const FLOAT_MULTIPLIER: f32 = 5.960_464_5E-8;

/// `java.util.Random`-compatible linear congruential generator, as used by
/// vanilla's `LegacyRandomSource`.
#[derive(Debug)]
pub struct LegacyRandomSource {
    seed: i64,
    gaussian: MarsagliaPolarGaussian,
}

impl LegacyRandomSource {
    pub fn new(seed: i64) -> Self {
        let mut source = Self {
            seed: 0,
            gaussian: MarsagliaPolarGaussian::new(),
        };
        source.set_seed(seed);
        source
    }

    /// `LegacyRandomSource.next(int)`.
    pub fn next(&mut self, bits: i32) -> i32 {
        let old_seed = self.seed;
        let new_seed = old_seed.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT) & MODULUS_MASK;
        self.seed = new_seed;
        (new_seed >> (MODULUS_BITS - bits)) as i32
    }

    /// `RandomSource.create(long)`'s legacy-compatible helper, exposed for tests
    /// and callers that need a seeded source without naming the type twice.
    pub fn create(seed: i64) -> Box<dyn RandomSource> {
        Box::new(Self::new(seed))
    }
}

impl RandomSource for LegacyRandomSource {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        Box::new(Self::new(self.next_long()))
    }

    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        Box::new(LegacyPositionalRandomFactory::new(self.next_long()))
    }

    fn set_seed(&mut self, seed: i64) {
        self.seed = (seed ^ MULTIPLIER) & MODULUS_MASK;
        self.gaussian.reset();
    }

    fn next_int(&mut self) -> i32 {
        self.next(32)
    }

    /// `BitRandomSource.nextInt(int)`: power-of-two fast path, otherwise the
    /// classic rejection loop.
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        if bound & bound.wrapping_neg() == bound {
            let product = (bound as i64) * (self.next(31) as i64);
            return (product >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound.wrapping_sub(1)) >= 0 {
                return value;
            }
        }
    }

    fn next_long(&mut self) -> i64 {
        let high = (self.next(32) as i64) << 32;
        high.wrapping_add(self.next(32) as i64)
    }

    fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }

    fn next_float(&mut self) -> f32 {
        self.next(24) as f32 * FLOAT_MULTIPLIER
    }

    fn next_double(&mut self) -> f64 {
        let high = (self.next(26) as i64) << 27;
        let low = self.next(27) as i64;
        (high + low) as f64 * DOUBLE_MULTIPLIER
    }

    fn next_gaussian(&mut self) -> f64 {
        let mut gaussian = std::mem::take(&mut self.gaussian);
        let value = gaussian.next_gaussian(self);
        self.gaussian = gaussian;
        value
    }
}

/// `LegacyRandomSource.LegacyPositionalRandomFactory`.
#[derive(Debug, Clone, Copy)]
pub struct LegacyPositionalRandomFactory {
    seed: i64,
}

impl LegacyPositionalRandomFactory {
    pub const fn new(seed: i64) -> Self {
        Self { seed }
    }

    pub const fn seed(&self) -> i64 {
        self.seed
    }
}

impl PositionalRandomFactory for LegacyPositionalRandomFactory {
    fn at(&self, x: i32, y: i32, z: i32) -> Box<dyn RandomSource> {
        Box::new(LegacyRandomSource::new(
            crate::block_seed(x, y, z) ^ self.seed,
        ))
    }

    /// Unlike the xoroshiro factory, the legacy one hashes with
    /// `String.hashCode()`, not MD5.
    fn from_hash_of(&self, name: &str) -> Box<dyn RandomSource> {
        Box::new(LegacyRandomSource::new(
            i64::from(crate::java_string_hash(name)) ^ self.seed,
        ))
    }

    fn from_seed(&self, seed: i64) -> Box<dyn RandomSource> {
        Box::new(LegacyRandomSource::new(seed))
    }
}
