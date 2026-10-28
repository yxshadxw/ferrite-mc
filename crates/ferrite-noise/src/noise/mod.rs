//! Ports of `net.minecraft.world.level.levelgen.synth` for Minecraft 26.3.

mod blended;
mod normal;
mod perlin;
mod simplex;
mod stack;

pub use blended::{BlendedNoise, FbmSet};
pub use normal::{NormalNoise, NormalNoiseBuilder, Normalization, Parameters};
pub use perlin::{PerlinNoise, SmearedPerlinNoise};
pub use simplex::SimplexNoise;
pub use stack::{NoiseStack, NoiseStackBuilder};

use crate::random::RandomSource;

/// A source of 2D/3D noise, mirroring `net.minecraft.world.level.levelgen.synth.Noise`.
pub trait Noise {
    fn get(&self, x: f64, y: f64, z: f64) -> f32;

    fn get_2d(&self, x: f64, z: f64) -> f32;
}

/// The vanilla gradient table (16 entries; only the first 12 are used by
/// `SimplexNoise`, all 16 by the Perlin family).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gradient {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Gradient {
    const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// `GradientNoise.Gradient.dot(float, float, float)`.
    pub fn dot(&self, dx: f32, dy: f32, dz: f32) -> f32 {
        (self.x as f32 * dx) + (self.y as f32 * dy) + (self.z as f32 * dz)
    }

    /// `GradientNoise.Gradient.dot(double, double, double)`.
    pub fn dot_f64(&self, dx: f64, dy: f64, dz: f64) -> f64 {
        (self.x as f64 * dx) + (self.y as f64 * dy) + (self.z as f64 * dz)
    }

    /// `GradientNoise.Gradient.dotXz(float, float)`.
    pub fn dot_xz(&self, dx: f32, dz: f32) -> f32 {
        (self.x as f32 * dx) + (self.z as f32 * dz)
    }
}

pub(crate) const GRADIENTS: [Gradient; 16] = [
    Gradient::new(1, 1, 0),
    Gradient::new(-1, 1, 0),
    Gradient::new(1, -1, 0),
    Gradient::new(-1, -1, 0),
    Gradient::new(1, 0, 1),
    Gradient::new(-1, 0, 1),
    Gradient::new(1, 0, -1),
    Gradient::new(-1, 0, -1),
    Gradient::new(0, 1, 1),
    Gradient::new(0, -1, 1),
    Gradient::new(0, 1, -1),
    Gradient::new(0, -1, -1),
    Gradient::new(1, 1, 0),
    Gradient::new(0, -1, 1),
    Gradient::new(-1, 1, 0),
    Gradient::new(0, -1, -1),
];

const ROUND_OFF: f64 = 33_554_432.0;
/// `Math.nextDown(1.6777216E7)`; const-evaluated from the exact bits.
const HALF_ROUND_OFF: f64 = f64::from_bits(16_777_216.0f64.to_bits() - 1);

/// `GradientNoise.wrap(double)`: folds coordinates into the +/-2^24 window.
pub(crate) fn wrap(value: f64) -> f64 {
    if (-HALF_ROUND_OFF..HALF_ROUND_OFF).contains(&value) {
        value
    } else {
        value - ((value / ROUND_OFF + 0.5).floor() * ROUND_OFF)
    }
}

/// Port of `net.minecraft.world.level.levelgen.synth.GradientNoise`.
#[derive(Clone, Debug)]
pub struct GradientNoise {
    pub(crate) perms: [u8; 256],
    pub(crate) offset_x: f64,
    pub(crate) offset_y: f64,
    pub(crate) offset_z: f64,
}

impl GradientNoise {
    pub(crate) fn new(random: &mut dyn RandomSource, offset_scale: f64) -> Self {
        let offset_x = random.next_double() * offset_scale;
        let offset_y = random.next_double() * offset_scale;
        let offset_z = random.next_double() * offset_scale;

        let mut perms = [0u8; 256];
        for (index, perm) in perms.iter_mut().enumerate() {
            *perm = index as u8;
        }
        for index in 0..256 {
            let swap_with = random.next_int_bound(256 - index as i32) as usize + index;
            perms.swap(index, swap_with);
        }

        Self {
            perms,
            offset_x,
            offset_y,
            offset_z,
        }
    }

    /// `GradientNoise.permute(int)`.
    pub(crate) fn permute(&self, value: i32) -> i32 {
        i32::from(self.perms[(value & 255) as usize])
    }

    /// `GradientNoise.gradDot(int, float, float, float)`.
    pub(crate) fn grad_dot(index: i32, dx: f32, dy: f32, dz: f32) -> f32 {
        GRADIENTS[(index & 15) as usize].dot(dx, dy, dz)
    }
}

/// `Mth.lerp(float, float, float)`.
pub(crate) fn lerp(delta: f32, start: f32, end: f32) -> f32 {
    start + delta * (end - start)
}

/// `Mth.lerp2(float, float, float, float, float, float)`.
pub(crate) fn lerp2(delta_x: f32, delta_y: f32, values: [f32; 4]) -> f32 {
    lerp(
        delta_y,
        lerp(delta_x, values[0], values[1]),
        lerp(delta_x, values[2], values[3]),
    )
}

/// `Mth.lerp3`: eight corner values in `[v000, v100, v010, v110, v001, v101, v011, v111]`
/// order, matching the argument order of the vanilla method.
pub(crate) fn lerp3(delta_x: f32, delta_y: f32, delta_z: f32, values: [f32; 8]) -> f32 {
    lerp(
        delta_z,
        lerp2(
            delta_x,
            delta_y,
            [values[0], values[1], values[2], values[3]],
        ),
        lerp2(
            delta_x,
            delta_y,
            [values[4], values[5], values[6], values[7]],
        ),
    )
}

/// `Mth.smoothstep(float)`.
pub(crate) fn smoothstep(value: f32) -> f32 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

#[cfg(test)]
mod tests {
    use super::{GRADIENTS, wrap};

    #[test]
    fn gradient_table_matches_vanilla() {
        assert_eq!(GRADIENTS.len(), 16);
        assert_eq!((GRADIENTS[0].x, GRADIENTS[0].y, GRADIENTS[0].z), (1, 1, 0));
        assert_eq!(
            (GRADIENTS[15].x, GRADIENTS[15].y, GRADIENTS[15].z),
            (0, -1, -1)
        );
    }

    #[test]
    fn wrap_is_identity_inside_window() {
        assert_eq!(wrap(0.0), 0.0);
        assert_eq!(wrap(-16_777_215.5), -16_777_215.5);
        assert_eq!(wrap(16_777_215.5), 16_777_215.5);
    }

    #[test]
    fn wrap_folds_large_coordinates() {
        let wrapped = wrap(33_554_433.5);
        assert!(wrapped.abs() < 16_777_216.0);
        assert_eq!(wrapped, 1.5);
    }
}
