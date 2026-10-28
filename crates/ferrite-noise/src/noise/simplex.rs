//! `net.minecraft.world.level.levelgen.synth.SimplexNoise`.

use super::{GRADIENTS, GradientNoise, Noise};
use crate::{floor_to_i32, random::RandomSource};

const SQRT_3: f64 = 1.732_050_807_568_877_2;
const F2: f64 = 0.5 * (SQRT_3 - 1.0);
const G2: f64 = (3.0 - SQRT_3) / 6.0;

/// `net.minecraft.world.level.levelgen.synth.SimplexNoise`.
#[derive(Clone, Debug)]
pub struct SimplexNoise {
    base: GradientNoise,
}

impl SimplexNoise {
    /// Plain constructor: offsets are drawn in `[0, 256)`.
    pub fn new(random: &mut dyn RandomSource) -> Self {
        Self {
            base: GradientNoise::new(random, 256.0),
        }
    }

    /// `SimplexNoise(RandomSource, boolean)`: vanilla passes `true` for legacy
    /// nether noise, which zeroes the coordinate offsets.
    pub fn new_legacy(random: &mut dyn RandomSource, legacy: bool) -> Self {
        Self {
            base: GradientNoise::new(random, if legacy { 0.0 } else { 256.0 }),
        }
    }

    /// `SimplexNoise.getCornerNoise3D(int, double, double, double, double)`.
    fn corner_noise_3d(&self, gradient: i32, x: f64, y: f64, z: f64, max: f64) -> f64 {
        let mut d = max - x * x - y * y - z * z;
        if d < 0.0 {
            0.0
        } else {
            d *= d;
            d * d * GRADIENTS[gradient as usize].dot_f64(x, y, z)
        }
    }

    pub fn get_2d(&self, x: f64, z: f64) -> f32 {
        let x = x + self.base.offset_x;
        let z = z + self.base.offset_y;

        let skew = (x + z) * F2;
        let i = floor_to_i32(x + skew);
        let j = floor_to_i32(z + skew);
        let unskew = f64::from(i + j) * G2;
        let x0 = f64::from(i) - unskew;
        let y0 = f64::from(j) - unskew;
        let x1 = x - x0;
        let y1 = z - y0;

        let (i1, j1) = if x1 > y1 { (1, 0) } else { (0, 1) };
        let x2 = x1 - f64::from(i1) + G2;
        let y2 = y1 - f64::from(j1) + G2;
        let x3 = x1 - 1.0 + 2.0 * G2;
        let y3 = y1 - 1.0 + 2.0 * G2;

        let ii = i & 255;
        let jj = j & 255;
        let gi0 = self.base.permute(ii + self.base.permute(jj)) % 12;
        let gi1 = self.base.permute(ii + i1 + self.base.permute(jj + j1)) % 12;
        let gi2 = self.base.permute(ii + 1 + self.base.permute(jj + 1)) % 12;

        let n0 = self.corner_noise_3d(gi0, x1, y1, 0.0, 0.5);
        let n1 = self.corner_noise_3d(gi1, x2, y2, 0.0, 0.5);
        let n2 = self.corner_noise_3d(gi2, x3, y3, 0.0, 0.5);
        (70.0 * (n0 + n1 + n2)) as f32
    }

    pub fn get_3d(&self, x: f64, y: f64, z: f64) -> f32 {
        let x = x + self.base.offset_x;
        let y = y + self.base.offset_y;
        let z = z + self.base.offset_z;

        let skew = (x + y + z) * (1.0 / 3.0);
        let i = floor_to_i32(x + skew);
        let j = floor_to_i32(y + skew);
        let k = floor_to_i32(z + skew);
        let unskew = f64::from(i + j + k) * (1.0 / 6.0);
        let x0 = f64::from(i) - unskew;
        let y0 = f64::from(j) - unskew;
        let z0 = f64::from(k) - unskew;
        let x1 = x - x0;
        let y1 = y - y0;
        let z1 = z - z0;

        let (i1, j1, k1, i2, j2, k2);
        if x1 >= y1 {
            if y1 >= z1 {
                (i1, j1, k1) = (1, 0, 0);
                (i2, j2, k2) = (1, 1, 0);
            } else if x1 >= z1 {
                (i1, j1, k1) = (1, 0, 0);
                (i2, j2, k2) = (1, 0, 1);
            } else {
                (i1, j1, k1) = (0, 0, 1);
                (i2, j2, k2) = (1, 0, 1);
            }
        } else if y1 < z1 {
            (i1, j1, k1) = (0, 0, 1);
            (i2, j2, k2) = (0, 1, 1);
        } else if x1 < z1 {
            (i1, j1, k1) = (0, 1, 0);
            (i2, j2, k2) = (0, 1, 1);
        } else {
            (i1, j1, k1) = (0, 1, 0);
            (i2, j2, k2) = (1, 1, 0);
        }

        let xa = x1 - f64::from(i1) + 1.0 / 6.0;
        let ya = y1 - f64::from(j1) + 1.0 / 6.0;
        let za = z1 - f64::from(k1) + 1.0 / 6.0;
        let xb = x1 - f64::from(i2) + 1.0 / 3.0;
        let yb = y1 - f64::from(j2) + 1.0 / 3.0;
        let zb = z1 - f64::from(k2) + 1.0 / 3.0;
        let xc = x1 - 1.0 + 0.5;
        let yc = y1 - 1.0 + 0.5;
        let zc = z1 - 1.0 + 0.5;

        let ii = i & 255;
        let jj = j & 255;
        let kk = k & 255;
        let gi0 = self
            .base
            .permute(ii + self.base.permute(jj + self.base.permute(kk)))
            % 12;
        let gi1 = self
            .base
            .permute(ii + i1 + self.base.permute(jj + j1 + self.base.permute(kk + k1)))
            % 12;
        let gi2 = self
            .base
            .permute(ii + i2 + self.base.permute(jj + j2 + self.base.permute(kk + k2)))
            % 12;
        let gi3 = self
            .base
            .permute(ii + 1 + self.base.permute(jj + 1 + self.base.permute(kk + 1)))
            % 12;

        let n0 = self.corner_noise_3d(gi0, x1, y1, z1, 0.6);
        let n1 = self.corner_noise_3d(gi1, xa, ya, za, 0.6);
        let n2 = self.corner_noise_3d(gi2, xb, yb, zb, 0.6);
        let n3 = self.corner_noise_3d(gi3, xc, yc, zc, 0.6);
        (32.0 * (n0 + n1 + n2 + n3)) as f32
    }
}

impl Noise for SimplexNoise {
    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        self.get_3d(x, y, z)
    }

    fn get_2d(&self, x: f64, z: f64) -> f32 {
        SimplexNoise::get_2d(self, x, z)
    }
}
