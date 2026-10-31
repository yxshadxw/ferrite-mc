//! Aquifer substance selection based on the 26.3 `Aquifer.NoiseBasedAquifer`.
use crate::{AquiferSettings, DensityFunction, ResolveError, SeededNoise, WorldgenData};
use ferrite_noise::{PositionalRandomFactory, RandomSource, XoroshiroRandomSource};
use std::collections::HashMap;

const OFFSETS: [(i32, i32); 13] = [
    (0, 0),
    (-2, -1),
    (-1, -1),
    (0, -1),
    (1, -1),
    (-3, 0),
    (-2, 0),
    (-1, 0),
    (1, 0),
    (-2, 1),
    (-1, 1),
    (0, 1),
    (1, 1),
];
const DRY: i32 = -32512;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fluid {
    Air,
    Water,
    Lava,
}
#[derive(Clone, Copy)]
struct Status {
    level: i32,
    fluid: Fluid,
}
impl Status {
    fn at(self, y: i32) -> Fluid {
        if y < self.level {
            self.fluid
        } else {
            Fluid::Air
        }
    }
}

pub(crate) struct AquiferSpec {
    barrier: DensityFunction,
    floodedness: DensityFunction,
    spread: DensityFunction,
    lava: DensityFunction,
    exclusion: DensityFunction,
    surface: DensityFunction,
}
impl AquiferSpec {
    pub(crate) fn new(config: &AquiferSettings, data: &WorldgenData) -> Result<Self, ResolveError> {
        Ok(Self {
            barrier: config.barrier.resolve_references(data)?,
            floodedness: config.fluid_level_floodedness.resolve_references(data)?,
            spread: config.fluid_level_spread.resolve_references(data)?,
            lava: config.lava.resolve_references(data)?,
            exclusion: config.exclusion.resolve_references(data)?,
            surface: config.surface_level.resolve_references(data)?,
        })
    }
}

pub(crate) struct Aquifer<'a> {
    spec: &'a AquiferSpec,
    noise: &'a SeededNoise,
    positional: Box<dyn PositionalRandomFactory>,
    sea: i32,
    skip_above: i32,
    positions: HashMap<(i32, i32, i32), (i32, i32, i32)>,
    statuses: HashMap<(i32, i32, i32), Status>,
    surfaces: HashMap<(i32, i32), i32>,
}
impl<'a> Aquifer<'a> {
    pub(crate) fn new(
        spec: &'a AquiferSpec,
        noise: &'a SeededNoise,
        seed: i64,
        sea: i32,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Self {
        let mut source = XoroshiroRandomSource::new(seed);
        let factory = source.fork_positional();
        let mut random = factory.from_hash_of("minecraft:aquifer");
        let positional = random.fork_positional();
        let mut result = Self {
            spec,
            noise,
            positional,
            sea,
            skip_above: i32::MAX,
            positions: HashMap::new(),
            statuses: HashMap::new(),
            surfaces: HashMap::new(),
        };
        let x = chunk_x * 16;
        let z = chunk_z * 16;
        let min_x = (x - 5).div_euclid(16) * 16;
        let min_z = (z - 5).div_euclid(16) * 16;
        let max_x = ((x + 15 - 5).div_euclid(16) + 1) * 16 + 9;
        let max_z = ((z + 15 - 5).div_euclid(16) + 1) * 16 + 9;
        let mut max_surface = i32::MIN;
        for zz in (min_z..=max_z & !3).step_by(4) {
            for xx in (min_x..=max_x & !3).step_by(4) {
                max_surface = max_surface.max(result.surface(xx, zz));
            }
        }
        result.skip_above = ((max_surface + 20).div_euclid(12) + 1) * 12 + 10;
        result
    }
    fn sample(&self, function: &DensityFunction, x: i32, y: i32, z: i32) -> f64 {
        function.compute(&self.noise.at(x, y, z))
    }
    fn global(&self, y: i32) -> Status {
        if y < -54.min(self.sea) {
            Status {
                level: -54,
                fluid: Fluid::Lava,
            }
        } else {
            Status {
                level: self.sea,
                fluid: Fluid::Water,
            }
        }
    }
    fn surface(&mut self, x: i32, z: i32) -> i32 {
        let key = (x & !3, z & !3);
        if let Some(value) = self.surfaces.get(&key) {
            return *value;
        }
        let value = self.sample(&self.spec.surface, key.0, 0, key.1).floor() as i32;
        self.surfaces.insert(key, value);
        value
    }
    fn location(&mut self, cell: (i32, i32, i32)) -> (i32, i32, i32) {
        if let Some(value) = self.positions.get(&cell) {
            return *value;
        }
        let mut random = self.positional.at(cell.0, cell.1, cell.2);
        let value = (
            cell.0 * 16 + random.next_int_bound(10),
            cell.1 * 12 + random.next_int_bound(9),
            cell.2 * 16 + random.next_int_bound(10),
        );
        self.positions.insert(cell, value);
        value
    }
    fn status(&mut self, cell: (i32, i32, i32)) -> Status {
        if let Some(value) = self.statuses.get(&cell) {
            return *value;
        }
        let (x, y, z) = self.location(cell);
        let global = self.global(y);
        let mut lowest = i32::MAX;
        let mut near_surface_water = false;
        for (dx, dz) in OFFSETS {
            let sx = x + dx * 16;
            let sz = z + dz * 16;
            let surface = self.surface(sx, sz);
            let adjusted = surface + 8;
            let center = dx == 0 && dz == 0;
            if center && y - 12 > adjusted {
                self.statuses.insert(cell, global);
                return global;
            }
            let above = y + 12 > adjusted;
            if (above || center) && self.global(adjusted).at(adjusted) != Fluid::Air {
                if center {
                    near_surface_water = true;
                }
                if above {
                    let result = self.global(adjusted);
                    self.statuses.insert(cell, result);
                    return result;
                }
            }
            lowest = lowest.min(surface);
        }
        let (partial, full) = if self.sample(&self.spec.exclusion, x, y, z) > 0.0 {
            (-1.0, -1.0)
        } else {
            let distance = lowest + 8 - y;
            let factor = if near_surface_water {
                1.0 - (f64::from(distance) / 64.0).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let value = self
                .sample(&self.spec.floodedness, x, y, z)
                .clamp(-1.0, 1.0);
            (value - (0.4 - 1.2 * factor), value - (0.8 - 1.1 * factor))
        };
        let level = if full > 0.0 {
            global.level
        } else if partial > 0.0 {
            let gx = x.div_euclid(16);
            let gy = y.div_euclid(40);
            let gz = z.div_euclid(16);
            let spread = (self.sample(&self.spec.spread, gx, gy, gz) as f32 * 10.0) as f64;
            (gy * 40 + 20 + ((spread / 3.0).floor() as i32) * 3).min(lowest)
        } else {
            DRY
        };
        let fluid = if level <= -10
            && level != DRY
            && global.fluid != Fluid::Lava
            && self
                .sample(
                    &self.spec.lava,
                    x.div_euclid(64),
                    y.div_euclid(40),
                    z.div_euclid(64),
                )
                .abs()
                > 0.3
        {
            Fluid::Lava
        } else {
            global.fluid
        };
        let value = Status { level, fluid };
        self.statuses.insert(cell, value);
        value
    }
    fn pressure(&self, x: i32, y: i32, z: i32, a: Status, b: Status) -> f64 {
        let fa = a.at(y);
        let fb = b.at(y);
        if (fa == Fluid::Water && fb == Fluid::Lava) || (fa == Fluid::Lava && fb == Fluid::Water) {
            return 2.0;
        }
        let difference = (a.level - b.level).abs();
        if difference == 0 {
            return 0.0;
        }
        let offset = f64::from(y) + 0.5 - f64::from(a.level + b.level) * 0.5;
        let edge = f64::from(difference) / 2.0 - offset.abs();
        let gradient = if offset > 0.0 {
            if edge > 0.0 { edge / 1.5 } else { edge / 2.5 }
        } else {
            let v = 3.0 + edge;
            if v > 0.0 { v / 3.0 } else { v / 10.0 }
        };
        let barrier = if (-2.0..=2.0).contains(&gradient) {
            self.sample(&self.spec.barrier, x, y, z)
        } else {
            0.0
        };
        2.0 * (barrier + gradient)
    }
    /// `None` is stone; a fluid result is the state occupying the block.
    pub(crate) fn substance(&mut self, x: i32, y: i32, z: i32, density: f64) -> Option<Fluid> {
        if density > 0.0 {
            return None;
        }
        let global = self.global(y);
        if y > self.skip_above {
            return Some(global.at(y));
        }
        if global.at(y) == Fluid::Lava {
            return Some(Fluid::Lava);
        }
        let anchor = (
            (x - 5).div_euclid(16),
            (y + 1).div_euclid(12),
            (z - 5).div_euclid(16),
        );
        let mut nearest: Vec<(i32, usize, (i32, i32, i32))> = Vec::with_capacity(12);
        for dx in 0..=1 {
            for dy in -1..=1 {
                for dz in 0..=1 {
                    let cell = (anchor.0 + dx, anchor.1 + dy, anchor.2 + dz);
                    let pos = self.location(cell);
                    let distance = (pos.0 - x).pow(2) + (pos.1 - y).pow(2) + (pos.2 - z).pow(2);
                    nearest.push((distance, nearest.len(), cell));
                }
            }
        }
        nearest.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
        let a = self.status(nearest[0].2);
        let similarity = |i: usize, j: usize| 1.0 - f64::from(nearest[j].0 - nearest[i].0) / 25.0;
        let sim12 = similarity(0, 1);
        let fluid = a.at(y);
        if sim12 <= 0.0 {
            return Some(fluid);
        }
        if fluid == Fluid::Water && self.global(y - 1).at(y - 1) == Fluid::Lava {
            return Some(fluid);
        }
        let b = self.status(nearest[1].2);
        if density + sim12 * self.pressure(x, y, z, a, b) > 0.0 {
            return None;
        }
        let c = self.status(nearest[2].2);
        let sim13 = similarity(0, 2);
        if sim13 > 0.0 && density + sim12 * sim13 * self.pressure(x, y, z, a, c) > 0.0 {
            return None;
        }
        let sim23 = similarity(1, 2);
        if sim23 > 0.0 && density + sim12 * sim23 * self.pressure(x, y, z, b, c) > 0.0 {
            return None;
        }
        Some(fluid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_java_aquifer_vectors() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        fn jars(dir: &std::path::Path, output: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    jars(&path, output);
                } else if path.extension().is_some_and(|ext| ext == "jar") {
                    output.push(path);
                }
            }
        }
        let default =
            std::path::PathBuf::from("C:/Program Files/Eclipse Adoptium/jdk-25.0.5.8-hotspot");
        let java_home = if default.is_dir() {
            default
        } else {
            std::path::PathBuf::from(std::env::var_os("JAVA_HOME").expect("Java 25 required"))
        };
        let mut dependencies = vec![root.join("vanilla/26.3/lib/server-26.3.jar")];
        jars(&root.join("vanilla/26.3/lib/libraries"), &mut dependencies);
        let separator = if cfg!(windows) { ";" } else { ":" };
        let cp = dependencies
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join(separator);
        let output = root.join("target/ferrite-worldgen-aquifer");
        std::fs::create_dir_all(&output).unwrap();
        let executable = |name: &str| {
            java_home.join("bin").join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.to_owned()
            })
        };
        let compile = std::process::Command::new(executable("javac"))
            .args(["-encoding", "UTF-8", "-cp", &cp, "-d"])
            .arg(&output)
            .arg(root.join("crates/ferrite-worldgen/tests/java/AquiferReference.java"))
            .output()
            .unwrap();
        assert!(
            compile.status.success(),
            "javac: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let vectors = output.join("aquifer.tsv");
        let classpath = format!("{}{separator}{cp}", output.display());
        let run = std::process::Command::new(executable("java"))
            .current_dir(&output)
            .args(["-cp", &classpath, "AquiferReference"])
            .arg(&vectors)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "java: {} {}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        let data = WorldgenData::load(root.join("vanilla/26.3/generated")).unwrap();
        let spec = AquiferSpec::new(
            data.noise_settings("overworld")
                .unwrap()
                .aquifers
                .as_ref()
                .unwrap(),
            &data,
        )
        .unwrap();
        let noise = SeededNoise::new(&data, -540760265);
        let mut aq = Aquifer::new(&spec, &noise, -540760265, 63, 0, 0);
        let rows = std::fs::read_to_string(vectors).unwrap();
        let mut failures = 0;
        for line in rows.lines() {
            let parts: Vec<_> = line.split('\t').collect();
            let x = parts[0].parse().unwrap();
            let y = parts[1].parse().unwrap();
            let z = parts[2].parse().unwrap();
            let d = parts[3].parse().unwrap();
            let expected: i32 = parts[4].parse().unwrap();
            let result = match aq.substance(x, y, z, d) {
                None => 0,
                Some(Fluid::Air) => 1,
                Some(Fluid::Water) => 2,
                Some(Fluid::Lava) => 3,
            };
            if result != expected {
                failures += 1;
                if failures <= 20 {
                    eprintln!("{x} {y} {z} {d}: {result} != {expected}");
                }
            }
        }
        assert_eq!(failures, 0);
    }
}
