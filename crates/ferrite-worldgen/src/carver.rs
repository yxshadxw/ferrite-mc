//! Cave carving masks based on 26.3 `CaveWorldCarver` and `WorldCarver`.
use ferrite_noise::{LegacyRandomSource, RandomSource};
use std::sync::OnceLock;

const TRIG_SCALE: f64 = 10430.378350470453;
fn sine(angle: f64, offset: f64) -> f32 {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        (0..65536)
            .map(|i| (f64::from(i) / TRIG_SCALE).sin() as f32)
            .collect()
    });
    table[((angle * TRIG_SCALE + offset) as i64 & 65535) as usize]
}
fn sin(v: f32) -> f32 {
    sine(f64::from(v), 0.0)
}
fn cos(v: f32) -> f32 {
    sine(f64::from(v), 16384.0)
}
fn uniform(r: &mut impl RandomSource, min: f32, max: f32) -> f32 {
    min + r.next_float() * (max - min)
}
fn feature_rng(seed: i64, index: i64, x: i32, z: i32) -> LegacyRandomSource {
    let seed = seed.wrapping_add(index);
    let mut rng = LegacyRandomSource::new(seed);
    let a = rng.next_long();
    let b = rng.next_long();
    rng.set_seed((i64::from(x).wrapping_mul(a)) ^ (i64::from(z).wrapping_mul(b)) ^ seed);
    rng
}

pub(crate) struct CarveMask {
    bits: Vec<bool>,
    min_y: i32,
    height: i32,
    chunk_x: i32,
    chunk_z: i32,
}
struct Tunnel {
    seed: i64,
    pos: [f64; 3],
    horizontal: f64,
    vertical: f64,
    thickness: f32,
    yaw: f32,
    pitch: f32,
    start: i32,
    distance: i32,
    scale: f64,
    floor: f64,
}
struct Ellipsoid<'a> {
    center: [f64; 3],
    radius: [f64; 2],
    floor: f64,
    widths: Option<&'a [f32]>,
}
impl CarveMask {
    pub(crate) fn new(seed: i64, chunk_x: i32, chunk_z: i32, min_y: i32, height: i32) -> Self {
        let mut mask = Self {
            bits: vec![false; 256 * height as usize],
            min_y,
            height,
            chunk_x,
            chunk_z,
        };
        for dx in -8..=8 {
            for dz in -8..=8 {
                let sx = chunk_x + dx;
                let sz = chunk_z + dz;
                for (idx, probability, ymax) in [(0, 0.15_f32, 180), (1, 0.07_f32, 47)] {
                    let mut random = feature_rng(seed, idx, sx, sz);
                    if random.next_float() <= probability {
                        mask.caves(&mut random, sx, sz, ymax);
                    }
                }
                let mut random = feature_rng(seed, 2, sx, sz);
                if random.next_float() <= 0.01 {
                    mask.canyon(&mut random, sx, sz);
                }
            }
        }
        mask
    }
    pub(crate) fn contains(&self, x: u8, y: i32, z: u8) -> bool {
        self.bits[((y - self.min_y) as usize) * 256 + usize::from(z) * 16 + usize::from(x)]
    }
    fn caves(&mut self, random: &mut LegacyRandomSource, source_x: i32, source_z: i32, ymax: i32) {
        let a = random.next_int_bound(15) + 1;
        let b = random.next_int_bound(a) + 1;
        let count = random.next_int_bound(b);
        for _ in 0..count {
            let x = f64::from(source_x * 16 + random.next_int_bound(16));
            let y = f64::from(self.min_y + 8 + random.next_int_bound(ymax - (self.min_y + 8) + 1));
            let z = f64::from(source_z * 16 + random.next_int_bound(16));
            let horizontal = f64::from(uniform(random, 0.7, 1.4));
            let vertical = f64::from(uniform(random, 0.8, 1.3));
            let floor = f64::from(uniform(random, -1.0, -0.4));
            let mut tunnels = 1;
            if random.next_int_bound(4) == 0 {
                let scale = f64::from(uniform(random, 0.1, 0.9));
                let thickness = 1.0 + random.next_float() * 6.0;
                let radius = 1.5 + f64::from(sin(std::f32::consts::FRAC_PI_2) * thickness);
                self.ellipsoid(Ellipsoid {
                    center: [x + 1.0, y, z],
                    radius: [radius, radius * scale],
                    floor,
                    widths: None,
                });
                tunnels += random.next_int_bound(4);
            }
            for _ in 0..tunnels {
                let yaw = random.next_float() * (std::f32::consts::PI * 2.0);
                let pitch = (random.next_float() - 0.5) / 4.0;
                let mut thickness = random.next_float() * 2.0 + random.next_float();
                if random.next_int_bound(10) == 0 {
                    thickness *= random.next_float() * random.next_float() * 3.0 + 1.0;
                }
                let distance = 112 - random.next_int_bound(28);
                let seed = random.next_long();
                self.tunnel(Tunnel {
                    seed,
                    pos: [x, y, z],
                    horizontal,
                    vertical,
                    thickness,
                    yaw,
                    pitch,
                    start: 0,
                    distance,
                    scale: 1.0,
                    floor,
                });
            }
        }
    }
    fn tunnel(&mut self, tunnel: Tunnel) {
        let Tunnel {
            seed,
            mut pos,
            horizontal,
            vertical,
            thickness,
            mut yaw,
            mut pitch,
            start,
            distance,
            scale,
            floor,
        } = tunnel;
        let mut random = LegacyRandomSource::new(seed);
        let split = random.next_int_bound(distance / 2) + distance / 4;
        let steep = random.next_int_bound(6) == 0;
        let (mut yaw_change, mut pitch_change) = (0.0_f32, 0.0_f32);
        for step in start..distance {
            let radius = 1.5
                + f64::from(sin(std::f32::consts::PI * step as f32 / distance as f32) * thickness);
            let vertical_radius = radius * scale;
            let pitch_cos = cos(pitch);
            pos[0] += f64::from(cos(yaw) * pitch_cos);
            pos[1] += f64::from(sin(pitch));
            pos[2] += f64::from(sin(yaw) * pitch_cos);
            pitch *= if steep { 0.92 } else { 0.7 };
            pitch += pitch_change * 0.1;
            yaw += yaw_change * 0.1;
            pitch_change *= 0.9;
            yaw_change *= 0.75;
            pitch_change += (random.next_float() - random.next_float()) * random.next_float() * 2.0;
            yaw_change += (random.next_float() - random.next_float()) * random.next_float() * 4.0;
            if step == split && thickness > 1.0 {
                for sign in [-1.0, 1.0] {
                    let seed = random.next_long();
                    let size = random.next_float() * 0.5 + 0.5;
                    self.tunnel(Tunnel {
                        seed,
                        pos,
                        horizontal,
                        vertical,
                        thickness: size,
                        yaw: yaw + sign * std::f32::consts::FRAC_PI_2,
                        pitch: pitch / 3.0,
                        start: step,
                        distance,
                        scale: 1.0,
                        floor,
                    });
                }
                return;
            }
            if random.next_int_bound(4) == 0 {
                continue;
            }
            let dx = pos[0] - f64::from(self.chunk_x * 16 + 8);
            let dz = pos[2] - f64::from(self.chunk_z * 16 + 8);
            let remaining = f64::from(distance - step);
            let max = f64::from(thickness + 2.0 + 16.0);
            if dx * dx + dz * dz - remaining * remaining > max * max {
                return;
            }
            self.ellipsoid(Ellipsoid {
                center: pos,
                radius: [radius * horizontal, vertical_radius * vertical],
                floor,
                widths: None,
            });
        }
    }
    fn canyon(&mut self, random: &mut LegacyRandomSource, sx: i32, sz: i32) {
        let x = f64::from(sx * 16 + random.next_int_bound(16));
        let y = f64::from(10 + random.next_int_bound(58));
        let z = f64::from(sz * 16 + random.next_int_bound(16));
        let yaw = random.next_float() * (std::f32::consts::PI * 2.0);
        let pitch = uniform(random, -0.125, 0.125);
        let thickness = random.next_float() * 4.0 + random.next_float() * 2.0;
        let distance = (112.0 * uniform(random, 0.75, 1.0)) as i32;
        let seed = random.next_long();
        self.canyon_tunnel(seed, [x, y, z], yaw, pitch, thickness, distance);
    }
    fn canyon_tunnel(
        &mut self,
        seed: i64,
        mut pos: [f64; 3],
        mut yaw: f32,
        mut pitch: f32,
        thickness: f32,
        distance: i32,
    ) {
        let mut random = LegacyRandomSource::new(seed);
        let mut width = 1.0_f32;
        let mut widths = vec![1.0_f32; self.height as usize];
        for (i, value) in widths.iter_mut().enumerate() {
            if i == 0 || random.next_int_bound(3) == 0 {
                width = 1.0 + random.next_float() * random.next_float();
            }
            *value = width * width;
        }
        let (mut yaw_change, mut pitch_change) = (0.0_f32, 0.0_f32);
        for step in 0..distance {
            let radius = 1.5
                + f64::from(sin(step as f32 * std::f32::consts::PI / distance as f32) * thickness);
            let horizontal = radius * f64::from(uniform(&mut random, 0.75, 1.0));
            let vertical = radius * 3.0 * f64::from(uniform(&mut random, 0.75, 1.0));
            let cos_pitch = cos(pitch);
            pos[0] += f64::from(cos(yaw) * cos_pitch);
            pos[1] += f64::from(sin(pitch));
            pos[2] += f64::from(sin(yaw) * cos_pitch);
            pitch *= 0.7;
            pitch += pitch_change * 0.05;
            yaw += yaw_change * 0.05;
            pitch_change *= 0.8;
            yaw_change *= 0.5;
            pitch_change += (random.next_float() - random.next_float()) * random.next_float() * 2.0;
            yaw_change += (random.next_float() - random.next_float()) * random.next_float() * 4.0;
            if random.next_int_bound(4) == 0 {
                continue;
            }
            let dx = pos[0] - f64::from(self.chunk_x * 16 + 8);
            let dz = pos[2] - f64::from(self.chunk_z * 16 + 8);
            let remaining = f64::from(distance - step);
            let max = f64::from(thickness + 2.0 + 16.0);
            if dx * dx + dz * dz - remaining * remaining > max * max {
                return;
            }
            self.ellipsoid(Ellipsoid {
                center: pos,
                radius: [horizontal, vertical],
                floor: -1.0,
                widths: Some(&widths),
            });
        }
    }
    fn ellipsoid(&mut self, ellipsoid: Ellipsoid<'_>) {
        let Ellipsoid {
            center: [x, y, z],
            radius: [rx, ry],
            floor,
            widths,
        } = ellipsoid;
        let x0 = self.chunk_x * 16;
        let z0 = self.chunk_z * 16;
        if (x - f64::from(x0 + 8)).abs() > 16.0 + rx * 2.0
            || (z - f64::from(z0 + 8)).abs() > 16.0 + rx * 2.0
        {
            return;
        }
        let minx = ((x - rx).floor() as i32 - x0 - 1).max(0);
        let maxx = ((x + rx).floor() as i32 - x0).min(15);
        let minz = ((z - rx).floor() as i32 - z0 - 1).max(0);
        let maxz = ((z + rx).floor() as i32 - z0).min(15);
        let miny = ((y - ry).floor() as i32 - 1).max(self.min_y + 1);
        let maxy = ((y + ry).floor() as i32 + 1).min(self.min_y + self.height - 8);
        for xx in minx..=maxx {
            for zz in minz..=maxz {
                let dx = (f64::from(x0 + xx) + 0.5 - x) / rx;
                let dz = (f64::from(z0 + zz) + 0.5 - z) / rx;
                if dx * dx + dz * dz >= 1.0 {
                    continue;
                }
                for yy in (miny + 1..=maxy).rev() {
                    let dy = (f64::from(yy) - 0.5 - y) / ry;
                    let accepted = if let Some(widths) = widths {
                        (dx * dx + dz * dz) * f64::from(widths[(yy - self.min_y - 1) as usize])
                            + dy * dy / 6.0
                            < 1.0
                    } else {
                        dy > floor && dx * dx + dy * dy + dz * dz < 1.0
                    };
                    if accepted {
                        self.bits
                            [(yy - self.min_y) as usize * 256 + zz as usize * 16 + xx as usize] =
                            true;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CarveMask;

    #[test]
    fn mask_matches_26_3_java_reference_vectors() {
        for (x, z, count, hash) in [
            (0, 0, 375, 378058343654617709_u64),
            (0, 1, 254, 8451383945342163025_u64),
            (0, 8, 679, 13670136765403772873_u64),
            (0, 15, 1851, 16857384617773292292_u64),
            (1, 0, 1307, 15948839474762625972_u64),
            (1, 1, 1125, 17865590824234822146_u64),
            (1, 8, 1396, 12651688505968832970_u64),
            (1, 15, 1405, 1582868609636194834_u64),
            (8, 0, 76, 3336729383384864814_u64),
            (8, 1, 0, 0_u64),
            (8, 8, 1072, 16451373834842736314_u64),
            (8, 15, 67, 16235775298695737805_u64),
            (15, 0, 1950, 8630309877051583301_u64),
            (15, 1, 1925, 17866671723293001020_u64),
            (15, 8, 476, 16960340424787670085_u64),
            (15, 15, 1137, 16460172573834085857_u64),
        ] {
            let mask = CarveMask::new(-540_760_265, x, z, -64, 384);
            let mut actual_count = 0;
            let mut actual_hash = 0_u64;
            for (index, set) in mask.bits.iter().enumerate() {
                if *set {
                    actual_count += 1;
                    actual_hash = actual_hash.wrapping_mul(31).wrapping_add(index as u64);
                }
            }
            assert_eq!((actual_count, actual_hash), (count, hash), "chunk {x},{z}");
        }
    }
}
