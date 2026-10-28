//! `PerlinNoise` and `SmearedPerlinNoise`.

use super::{GradientNoise, Noise, lerp3, smoothstep, wrap};
use crate::{floor_to_i32, random::RandomSource};

/// `net.minecraft.world.level.levelgen.synth.PerlinNoise`: a single-octave
/// gradient noise with cubic smoothing.
#[derive(Clone, Debug)]
pub struct PerlinNoise {
    pub(crate) base: GradientNoise,
}

impl PerlinNoise {
    pub fn new(random: &mut dyn RandomSource) -> Self {
        Self {
            base: GradientNoise::new(random, 256.0),
        }
    }

    pub fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let x = wrap(x) + self.base.offset_x;
        let y = wrap(y) + self.base.offset_y;
        let z = wrap(z) + self.base.offset_z;

        let x_floor = floor_to_i32(x);
        let y_floor = floor_to_i32(y);
        let z_floor = floor_to_i32(z);

        let delta_x = (x - f64::from(x_floor)) as f32;
        let delta_y = (y - f64::from(y_floor)) as f32;
        let delta_z = (z - f64::from(z_floor)) as f32;

        self.sample_and_lerp(
            [x_floor, y_floor, z_floor],
            [delta_x, delta_y, delta_z],
            delta_y,
        )
    }

    /// `PerlinNoise.sampleAndLerp(int, int, int, float, float, float, float)`.
    /// The last argument is the (possibly vertically fudged) value used to
    /// smooth the Y axis; `SmearedPerlinNoise` passes the raw fraction there.
    pub(crate) fn sample_and_lerp(&self, block: [i32; 3], delta: [f32; 3], y_lerp: f32) -> f32 {
        let [x, y, z] = block;
        let [delta_x, delta_y, delta_z] = delta;
        let x_a = self.base.permute(x);
        let x_b = self.base.permute(x + 1);
        let y_xa = self.base.permute(x_a + y);
        let y_xa_1 = self.base.permute(x_a + y + 1);
        let y_xb = self.base.permute(x_b + y);
        let y_xb_1 = self.base.permute(x_b + y + 1);

        let n000 = GradientNoise::grad_dot(self.base.permute(y_xa + z), delta_x, delta_y, delta_z);
        let n100 =
            GradientNoise::grad_dot(self.base.permute(y_xb + z), delta_x - 1.0, delta_y, delta_z);
        let n010 = GradientNoise::grad_dot(
            self.base.permute(y_xa_1 + z),
            delta_x,
            delta_y - 1.0,
            delta_z,
        );
        let n110 = GradientNoise::grad_dot(
            self.base.permute(y_xb_1 + z),
            delta_x - 1.0,
            delta_y - 1.0,
            delta_z,
        );
        let n001 = GradientNoise::grad_dot(
            self.base.permute(y_xa + z + 1),
            delta_x,
            delta_y,
            delta_z - 1.0,
        );
        let n101 = GradientNoise::grad_dot(
            self.base.permute(y_xb + z + 1),
            delta_x - 1.0,
            delta_y,
            delta_z - 1.0,
        );
        let n011 = GradientNoise::grad_dot(
            self.base.permute(y_xa_1 + z + 1),
            delta_x,
            delta_y - 1.0,
            delta_z - 1.0,
        );
        let n111 = GradientNoise::grad_dot(
            self.base.permute(y_xb_1 + z + 1),
            delta_x - 1.0,
            delta_y - 1.0,
            delta_z - 1.0,
        );

        lerp3(
            smoothstep(delta_x),
            smoothstep(y_lerp),
            smoothstep(delta_z),
            [n000, n100, n010, n110, n001, n101, n011, n111],
        )
    }
}

impl Noise for PerlinNoise {
    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        PerlinNoise::get(self, x, y, z)
    }

    fn get_2d(&self, x: f64, z: f64) -> f32 {
        self.get(x, 0.0, z)
    }
}

/// `net.minecraft.world.level.levelgen.synth.SmearedPerlinNoise`: a Perlin
/// octave whose vertical sampling is snapped to `fudge_y_scale` bands. This is
/// the 26.3 replacement for the old `ImprovedNoise.noise(x, y, z, yScale, yMax)`
/// variant.
#[derive(Clone, Debug)]
pub struct SmearedPerlinNoise {
    perlin: PerlinNoise,
    fudge_y_scale: f64,
}

impl SmearedPerlinNoise {
    pub fn new(random: &mut dyn RandomSource, fudge_y_scale: f64) -> Self {
        Self {
            perlin: PerlinNoise::new(random),
            fudge_y_scale,
        }
    }

    pub fn fudge_y_scale(&self) -> f64 {
        self.fudge_y_scale
    }

    pub fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let base = &self.perlin.base;
        // `computeFudgeY` is called with the original Y coordinate, not the
        // wrapped/offset one.
        let raw_y = y;
        let x = wrap(x) + base.offset_x;
        let y = wrap(y) + base.offset_y;
        let z = wrap(z) + base.offset_z;

        let x_floor = floor_to_i32(x);
        let y_floor = floor_to_i32(y);
        let z_floor = floor_to_i32(z);

        let delta_x = (x - f64::from(x_floor)) as f32;
        let y_fraction = y - f64::from(y_floor);
        let delta_z = (z - f64::from(z_floor)) as f32;
        let delta_y = (y_fraction - self.compute_fudge_y(raw_y, y_fraction)) as f32;

        self.perlin.sample_and_lerp(
            [x_floor, y_floor, z_floor],
            [delta_x, delta_y, delta_z],
            y_fraction as f32,
        )
    }

    /// `SmearedPerlinNoise.computeFudgeY(double, double)`.
    ///
    /// The second argument is the Y fraction, not the fudge scale: vanilla
    /// picks the raw coordinate when it lies in `[0, yFraction)`, and otherwise
    /// the fraction.
    fn compute_fudge_y(&self, y: f64, y_fraction: f64) -> f64 {
        let value = if y >= 0.0 && y < y_fraction {
            y
        } else {
            y_fraction
        };
        (value / self.fudge_y_scale + 1.000_000_011_686_097_4E-7).floor() * self.fudge_y_scale
    }
}

impl Noise for SmearedPerlinNoise {
    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        SmearedPerlinNoise::get(self, x, y, z)
    }

    fn get_2d(&self, x: f64, z: f64) -> f32 {
        self.get(x, 0.0, z)
    }
}
