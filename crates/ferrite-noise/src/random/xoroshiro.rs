//! `net.minecraft.world.level.levelgen.XoroshiroRandomSource`,
//! `Xoroshiro128PlusPlus`, and the matching positional random factory.

use super::{
    MarsagliaPolarGaussian, PositionalRandomFactory, RandomSource, RandomSupport, Seed128bit,
};

const FLOAT_UNIT: f32 = 5.960_464_5E-8;
const DOUBLE_UNIT: f64 = 1.110_223_024_625_156_5E-16;

/// `Xoroshiro128PlusPlus`.
#[derive(Debug, Clone, Copy)]
pub struct Xoroshiro128PlusPlus {
    seed_lo: i64,
    seed_hi: i64,
}

impl Xoroshiro128PlusPlus {
    pub fn new(seed_lo: i64, seed_hi: i64) -> Self {
        if seed_lo | seed_hi == 0 {
            Self {
                seed_lo: RandomSupport::GOLDEN_RATIO_64,
                seed_hi: RandomSupport::SILVER_RATIO_64,
            }
        } else {
            Self { seed_lo, seed_hi }
        }
    }

    pub fn from_seed(seed: Seed128bit) -> Self {
        Self::new(seed.seed_lo, seed.seed_hi)
    }

    pub fn next_long(&mut self) -> i64 {
        let seed_lo = self.seed_lo;
        let seed_hi = self.seed_hi;
        let result = seed_lo
            .wrapping_add(seed_hi)
            .rotate_left(17)
            .wrapping_add(seed_lo);
        let seed_hi = seed_hi ^ seed_lo;
        self.seed_lo = seed_lo.rotate_left(49) ^ seed_hi ^ (seed_hi << 21);
        self.seed_hi = seed_hi.rotate_left(28);
        result
    }
}

/// `XoroshiroRandomSource`.
#[derive(Debug)]
pub struct XoroshiroRandomSource {
    random_number_generator: Xoroshiro128PlusPlus,
    gaussian: MarsagliaPolarGaussian,
}

impl XoroshiroRandomSource {
    /// `XoroshiroRandomSource(long)`: upgrades the 64-bit seed through
    /// `RandomSupport.upgradeSeedTo128bit`.
    pub fn new(seed: i64) -> Self {
        Self::from_generator(Xoroshiro128PlusPlus::from_seed(
            RandomSupport::upgrade_seed_to_128bit(seed),
        ))
    }

    /// `XoroshiroRandomSource(RandomSupport.Seed128bit)`: no extra mixing.
    pub fn from_seed(seed: Seed128bit) -> Self {
        Self::from_generator(Xoroshiro128PlusPlus::from_seed(seed))
    }

    /// `XoroshiroRandomSource(long, long)`: raw 128-bit state.
    pub fn new_with_seeds(seed_lo: i64, seed_hi: i64) -> Self {
        Self::from_generator(Xoroshiro128PlusPlus::new(seed_lo, seed_hi))
    }

    fn from_generator(random_number_generator: Xoroshiro128PlusPlus) -> Self {
        Self {
            random_number_generator,
            gaussian: MarsagliaPolarGaussian::new(),
        }
    }

    fn next_bits(&mut self, bits: i32) -> i64 {
        ((self.random_number_generator.next_long() as u64) >> (64 - bits)) as i64
    }
}

impl RandomSource for XoroshiroRandomSource {
    fn fork(&mut self) -> Box<dyn RandomSource> {
        let seed_lo = self.random_number_generator.next_long();
        let seed_hi = self.random_number_generator.next_long();
        Box::new(Self::new_with_seeds(seed_lo, seed_hi))
    }

    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory> {
        let seed_lo = self.random_number_generator.next_long();
        let seed_hi = self.random_number_generator.next_long();
        Box::new(XoroshiroPositionalRandomFactory::new(seed_lo, seed_hi))
    }

    fn set_seed(&mut self, seed: i64) {
        self.random_number_generator =
            Xoroshiro128PlusPlus::from_seed(RandomSupport::upgrade_seed_to_128bit(seed));
        self.gaussian.reset();
    }

    fn next_int(&mut self) -> i32 {
        self.random_number_generator.next_long() as i32
    }

    /// `XoroshiroRandomSource.nextInt(int)`: Lemire's multiply-and-reject
    /// method, matching vanilla exactly.
    fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        let mut value = self.next_int() as u32 as u64;
        let mut product = value * bound as u64;
        let mut low = product & 0xFFFF_FFFF;
        if low < bound as u64 {
            let threshold = (bound.wrapping_neg() as u32 % bound as u32) as u64;
            while low < threshold {
                value = self.next_int() as u32 as u64;
                product = value * bound as u64;
                low = product & 0xFFFF_FFFF;
            }
        }
        (product >> 32) as i32
    }

    fn next_long(&mut self) -> i64 {
        self.random_number_generator.next_long()
    }

    fn next_boolean(&mut self) -> bool {
        self.random_number_generator.next_long() & 1 != 0
    }

    fn next_float(&mut self) -> f32 {
        self.next_bits(24) as f32 * FLOAT_UNIT
    }

    fn next_double(&mut self) -> f64 {
        self.next_bits(53) as f64 * DOUBLE_UNIT
    }

    fn next_gaussian(&mut self) -> f64 {
        let mut gaussian = std::mem::take(&mut self.gaussian);
        let value = gaussian.next_gaussian(self);
        self.gaussian = gaussian;
        value
    }

    fn consume_count(&mut self, count: i32) {
        for _ in 0..count {
            self.random_number_generator.next_long();
        }
    }
}

/// `XoroshiroRandomSource.XoroshiroPositionalRandomFactory`.
#[derive(Debug, Clone, Copy)]
pub struct XoroshiroPositionalRandomFactory {
    seed_lo: i64,
    seed_hi: i64,
}

impl XoroshiroPositionalRandomFactory {
    pub const fn new(seed_lo: i64, seed_hi: i64) -> Self {
        Self { seed_lo, seed_hi }
    }

    pub const fn seed_lo(&self) -> i64 {
        self.seed_lo
    }

    pub const fn seed_hi(&self) -> i64 {
        self.seed_hi
    }
}

impl PositionalRandomFactory for XoroshiroPositionalRandomFactory {
    fn at(&self, x: i32, y: i32, z: i32) -> Box<dyn RandomSource> {
        let seed_lo = crate::block_seed(x, y, z) ^ self.seed_lo;
        Box::new(XoroshiroRandomSource::new_with_seeds(seed_lo, self.seed_hi))
    }

    fn from_hash_of(&self, name: &str) -> Box<dyn RandomSource> {
        let hashed = RandomSupport::seed_from_hash_of(name);
        Box::new(XoroshiroRandomSource::from_seed(
            hashed.xor(self.seed_lo, self.seed_hi),
        ))
    }

    fn from_seed(&self, seed: i64) -> Box<dyn RandomSource> {
        Box::new(XoroshiroRandomSource::new_with_seeds(
            seed ^ self.seed_lo,
            seed ^ self.seed_hi,
        ))
    }
}
