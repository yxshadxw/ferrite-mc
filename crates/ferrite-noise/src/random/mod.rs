//! Ports of `net.minecraft.util.RandomSource` and `net.minecraft.world.level.levelgen.RandomSupport`.

mod legacy;
mod md5;
mod xoroshiro;

pub use legacy::{LegacyPositionalRandomFactory, LegacyRandomSource};
pub use xoroshiro::{XoroshiroPositionalRandomFactory, XoroshiroRandomSource};

/// The subset of `net.minecraft.util.RandomSource` used by world generation.
pub trait RandomSource {
    fn fork(&mut self) -> Box<dyn RandomSource>;

    fn fork_positional(&mut self) -> Box<dyn PositionalRandomFactory>;

    fn set_seed(&mut self, seed: i64);

    fn next_int(&mut self) -> i32;

    fn next_int_bound(&mut self, bound: i32) -> i32;

    fn next_long(&mut self) -> i64;

    fn next_boolean(&mut self) -> bool;

    fn next_float(&mut self) -> f32;

    fn next_double(&mut self) -> f64;

    fn next_gaussian(&mut self) -> f64;

    /// `RandomSource.consumeCount` default: discard `count` `nextInt()` calls.
    fn consume_count(&mut self, count: i32) {
        for _ in 0..count {
            self.next_int();
        }
    }

    /// `RandomSource.nextIntBetweenInclusive`.
    fn next_int_between_inclusive(&mut self, min: i32, max: i32) -> i32 {
        min + self.next_int_bound(max - min + 1)
    }
}

/// Port of `net.minecraft.world.level.levelgen.PositionalRandomFactory`.
///
/// The `from_*` naming mirrors the Java factory methods on purpose, so the
/// usual Rust `from_*` constructor convention does not apply.
#[allow(clippy::wrong_self_convention)]
pub trait PositionalRandomFactory {
    fn at(&self, x: i32, y: i32, z: i32) -> Box<dyn RandomSource>;

    fn from_hash_of(&self, name: &str) -> Box<dyn RandomSource>;

    fn from_seed(&self, seed: i64) -> Box<dyn RandomSource>;
}

/// Port of `net.minecraft.world.level.levelgen.MarsagliaPolarGaussian`.
#[derive(Debug, Default)]
pub struct MarsagliaPolarGaussian {
    next_next_gaussian: f64,
    have_next_next_gaussian: bool,
}

impl MarsagliaPolarGaussian {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.have_next_next_gaussian = false;
    }

    pub fn next_gaussian(&mut self, random: &mut dyn RandomSource) -> f64 {
        if self.have_next_next_gaussian {
            self.have_next_next_gaussian = false;
            return self.next_next_gaussian;
        }

        loop {
            let d = 2.0 * random.next_double() - 1.0;
            let e = 2.0 * random.next_double() - 1.0;
            let f = d * d + e * e;
            if f < 1.0 && f != 0.0 {
                let g = (-2.0 * crate::math::log(f) / f).sqrt();
                self.next_next_gaussian = e * g;
                self.have_next_next_gaussian = true;
                return d * g;
            }
        }
    }
}

/// A 128-bit seed, mirroring `RandomSupport.Seed128bit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seed128bit {
    pub seed_lo: i64,
    pub seed_hi: i64,
}

impl Seed128bit {
    pub const fn new(seed_lo: i64, seed_hi: i64) -> Self {
        Self { seed_lo, seed_hi }
    }

    pub const fn xor(self, seed_lo: i64, seed_hi: i64) -> Self {
        Self {
            seed_lo: self.seed_lo ^ seed_lo,
            seed_hi: self.seed_hi ^ seed_hi,
        }
    }

    /// `RandomSupport.Seed128bit.mixed`.
    pub fn mixed(self) -> Self {
        Self {
            seed_lo: RandomSupport::mix_stafford_13(self.seed_lo),
            seed_hi: RandomSupport::mix_stafford_13(self.seed_hi),
        }
    }
}

/// Port of `net.minecraft.world.level.levelgen.RandomSupport`.
pub struct RandomSupport;

impl RandomSupport {
    pub const GOLDEN_RATIO_64: i64 = -7_046_029_254_386_353_131;
    pub const SILVER_RATIO_64: i64 = 7_640_891_576_956_012_809;

    /// `RandomSupport.mixStafford13`: the Stafford variant 13 finaliser.
    pub fn mix_stafford_13(mut value: i64) -> i64 {
        value ^= ((value as u64) >> 30) as i64;
        value = value.wrapping_mul(-4_658_895_280_553_007_687);
        value ^= ((value as u64) >> 27) as i64;
        value = value.wrapping_mul(-7_723_592_293_110_705_685);
        value ^= ((value as u64) >> 31) as i64;
        value
    }

    /// `RandomSupport.upgradeSeedTo128bitUnmixed`.
    pub fn upgrade_seed_to_128bit_unmixed(seed: i64) -> Seed128bit {
        let seed_lo = seed ^ Self::SILVER_RATIO_64;
        let seed_hi = seed_lo.wrapping_add(Self::GOLDEN_RATIO_64);
        Seed128bit::new(seed_lo, seed_hi)
    }

    /// `RandomSupport.upgradeSeedTo128bit`.
    pub fn upgrade_seed_to_128bit(seed: i64) -> Seed128bit {
        Self::upgrade_seed_to_128bit_unmixed(seed).mixed()
    }

    /// `RandomSupport.seedFromHashOf`: MD5 of the UTF-8 bytes, split into two
    /// big-endian `i64` halves (Guava's `Longs.fromBytes` is network order).
    pub fn seed_from_hash_of(name: &str) -> Seed128bit {
        let digest = md5::md5(name.as_bytes());
        let seed_lo = i64::from_be_bytes(digest[0..8].try_into().expect("MD5 digest is 16 bytes"));
        let seed_hi = i64::from_be_bytes(digest[8..16].try_into().expect("MD5 digest is 16 bytes"));
        Seed128bit::new(seed_lo, seed_hi)
    }
}

#[cfg(test)]
mod tests {
    use super::RandomSupport;

    #[test]
    fn md5_matches_rfc_1321_vectors() {
        assert_eq!(
            super::md5::md5(b""),
            [
                0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8,
                0x42, 0x7e
            ]
        );
        assert_eq!(
            super::md5::md5(b"abc"),
            [
                0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1,
                0x7f, 0x72
            ]
        );
    }

    #[test]
    fn seed_upgrade_is_deterministic() {
        let seed = RandomSupport::upgrade_seed_to_128bit(42);
        assert_eq!(seed, RandomSupport::upgrade_seed_to_128bit(42));
        assert_ne!(
            seed,
            RandomSupport::upgrade_seed_to_128bit_unmixed(42),
            "upgradeSeedTo128bit mixes the unmixed seed"
        );
    }
}
