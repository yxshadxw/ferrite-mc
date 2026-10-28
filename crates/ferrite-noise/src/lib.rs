//! Bit-exact ports of the Minecraft Java 26.3 world-generation random and noise
//! primitives.
//!
//! The code follows the unobfuscated 26.3 server classes one-for-one:
//!
//! | Vanilla class | This crate |
//! |---|---|
//! | `LegacyRandomSource` | [`LegacyRandomSource`] |
//! | `XoroshiroRandomSource` / `Xoroshiro128PlusPlus` | [`XoroshiroRandomSource`] |
//! | `LegacyRandomSource.LegacyPositionalRandomFactory` | [`LegacyPositionalRandomFactory`] |
//! | `XoroshiroRandomSource.XoroshiroPositionalRandomFactory` | [`XoroshiroPositionalRandomFactory`] |
//! | `RandomSupport` | [`RandomSupport`] |
//! | `GradientNoise` | [`GradientNoise`] |
//! | `PerlinNoise` | [`PerlinNoise`] |
//! | `SmearedPerlinNoise` | [`SmearedPerlinNoise`] |
//! | `SimplexNoise` | [`SimplexNoise`] |
//! | `NoiseStack` | [`NoiseStack`] |
//! | `NormalNoise` | [`NormalNoise`] |
//! | `BlendedNoise` | [`BlendedNoise`] |
//! | `LegacyFbmInitializer` | [`NoiseStack::create_for_legacy_nether_biome`] |
//!
//! A note on names: older versions (1.16–1.20) called the single-octave gradient
//! noise `ImprovedNoise`, and its `noise(x, y, z, yScale, yMax)` variant applied
//! a vertical fudge. In 26.3 that class was split into [`PerlinNoise`] (plain
//! samples) and [`SmearedPerlinNoise`] (vertical fudge, y scale only); octave
//! stacking moved into [`NoiseStack`] and [`NormalNoise`]. Because vanilla
//! behaviour is defined by the 26.3 classes, this crate implements those and not
//! the retired 1.18-era API.
//!
//! Doubles and floats are produced with the same IEEE-754 operations in the same
//! order as the Java code, so samples are bitwise identical. The integration test
//! under `tests/reference.rs` checks that against values dumped from the local
//! 26.3 server jar.

#![forbid(unsafe_code)]

mod math;
pub mod noise;
pub mod random;

pub use noise::{
    BlendedNoise, Gradient, GradientNoise, Noise, NoiseStack, NormalNoise, PerlinNoise,
    SimplexNoise, SmearedPerlinNoise,
};
pub use random::{
    LegacyPositionalRandomFactory, LegacyRandomSource, MarsagliaPolarGaussian,
    PositionalRandomFactory, RandomSource, RandomSupport, Seed128bit,
    XoroshiroPositionalRandomFactory, XoroshiroRandomSource,
};

/// Computes Java's `String.hashCode()` (over UTF-16 code units, wrapping `i32`).
pub(crate) fn java_string_hash(value: &str) -> i32 {
    let mut hash: i32 = 0;
    for unit in value.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash
}

/// Java's `Mth.floor(double)`, including the saturating `double`-to-`int` cast.
pub(crate) fn floor_to_i32(value: f64) -> i32 {
    value.floor() as i32
}

/// Java's `Mth.getSeed(int, int, int)`.
pub(crate) fn block_seed(x: i32, y: i32, z: i32) -> i64 {
    let mut seed =
        (x.wrapping_mul(3_129_871) as i64) ^ ((z as i64).wrapping_mul(116_129_781)) ^ (y as i64);
    seed = seed
        .wrapping_mul(seed)
        .wrapping_mul(42_317_861)
        .wrapping_add(seed.wrapping_mul(11));
    seed >> 16
}

#[cfg(test)]
mod tests {
    use super::{block_seed, java_string_hash};

    #[test]
    fn java_string_hash_matches_known_values() {
        assert_eq!(java_string_hash(""), 0);
        assert_eq!(java_string_hash("abc"), 96354);
        assert_eq!(java_string_hash("minecraft:overworld"), 1_104_210_353);
        assert_eq!(java_string_hash("octave_-8"), 440_898_202);
    }

    #[test]
    fn block_seed_matches_vanilla_values() {
        assert_eq!(block_seed(0, 0, 0), 0);
        assert_eq!(block_seed(1, 2, 3), -33_674_130_277_896);
        assert_eq!(block_seed(-17, 64, 25), 13_851_547_012_154);
    }
}
