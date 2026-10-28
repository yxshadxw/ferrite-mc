//! Bit-exact differential test against values dumped from the local Minecraft
//! 26.3 server jar by `tests/java/NoiseReference.java`.
//!
//! The fixture lives in `target/noise-fixtures/` and is regenerated whenever the
//! harness or the server jar is newer. If Java or the jar is unavailable the
//! test skips with a message instead of failing, so clean checkouts still build.

use std::path::{Path, PathBuf};
use std::process::Command;

use ferrite_noise::noise::{Noise, NormalNoise};
use ferrite_noise::{
    BlendedNoise, LegacyPositionalRandomFactory, LegacyRandomSource, PerlinNoise,
    PositionalRandomFactory, RandomSource, RandomSupport, SimplexNoise, SmearedPerlinNoise,
    XoroshiroPositionalRandomFactory, XoroshiroRandomSource,
};
use serde_json::Value;

const SAMPLES: usize = 10_000;
const LEGACY_SEED: i64 = 0x0123_4567_89AB_CDEF;
const XOROSHIRO_SEED: i64 = 0x0FED_CBA9_8765_4321;

#[test]
fn reference_samples_are_bitwise_identical() {
    let Some(fixture) = load_fixture() else {
        return;
    };

    let mut report: Vec<(&'static str, usize)> = Vec::new();
    let check = |name: &'static str,
                 expected: &Value,
                 actual: Vec<u64>,
                 report: &mut Vec<(&'static str, usize)>| {
        report.push((name, compare_bits(name, expected, actual)));
    };

    // ---- random sources ---------------------------------------------------
    let legacy = &fixture["legacy_random_sequences"];
    let mut source = LegacyRandomSource::new(LEGACY_SEED);
    check(
        "legacy.next_int",
        &legacy["next_int"],
        (0..SAMPLES)
            .map(|_| source.next_int() as u32 as u64)
            .collect(),
        &mut report,
    );
    let mut source = LegacyRandomSource::new(LEGACY_SEED);
    check(
        "legacy.next_int_bound(1000)",
        &legacy["next_int_bound_1000"],
        (0..SAMPLES)
            .map(|_| source.next_int_bound(1000) as u32 as u64)
            .collect(),
        &mut report,
    );
    let mut source = LegacyRandomSource::new(LEGACY_SEED);
    check(
        "legacy.next_long",
        &legacy["next_long"],
        (0..SAMPLES).map(|_| source.next_long() as u64).collect(),
        &mut report,
    );
    let mut source = LegacyRandomSource::new(LEGACY_SEED);
    check(
        "legacy.next_double",
        &legacy["next_double"],
        (0..SAMPLES)
            .map(|_| source.next_double().to_bits())
            .collect(),
        &mut report,
    );
    let mut source = LegacyRandomSource::new(LEGACY_SEED);
    check(
        "legacy.next_float",
        &legacy["next_float"],
        (0..SAMPLES)
            .map(|_| source.next_float().to_bits() as u64)
            .collect(),
        &mut report,
    );
    let mut source = LegacyRandomSource::new(LEGACY_SEED);
    check(
        "legacy.next_gaussian",
        &legacy["next_gaussian"],
        (0..SAMPLES)
            .map(|_| source.next_gaussian().to_bits())
            .collect(),
        &mut report,
    );
    let mut source = LegacyRandomSource::new(LEGACY_SEED);
    check(
        "legacy.next_boolean",
        &legacy["next_boolean"],
        (0..SAMPLES)
            .map(|_| u64::from(source.next_boolean()))
            .collect(),
        &mut report,
    );

    let xoroshiro = &fixture["xoroshiro_random_sequences"];
    let mut source = XoroshiroRandomSource::new(XOROSHIRO_SEED);
    check(
        "xoroshiro.next_int",
        &xoroshiro["next_int"],
        (0..SAMPLES)
            .map(|_| source.next_int() as u32 as u64)
            .collect(),
        &mut report,
    );
    let mut source = XoroshiroRandomSource::new(XOROSHIRO_SEED);
    check(
        "xoroshiro.next_int_bound(1000)",
        &xoroshiro["next_int_bound_1000"],
        (0..SAMPLES)
            .map(|_| source.next_int_bound(1000) as u32 as u64)
            .collect(),
        &mut report,
    );
    let mut source = XoroshiroRandomSource::new(XOROSHIRO_SEED);
    check(
        "xoroshiro.next_long",
        &xoroshiro["next_long"],
        (0..SAMPLES).map(|_| source.next_long() as u64).collect(),
        &mut report,
    );
    let mut source = XoroshiroRandomSource::new(XOROSHIRO_SEED);
    check(
        "xoroshiro.next_double",
        &xoroshiro["next_double"],
        (0..SAMPLES)
            .map(|_| source.next_double().to_bits())
            .collect(),
        &mut report,
    );
    let mut source = XoroshiroRandomSource::new(XOROSHIRO_SEED);
    check(
        "xoroshiro.next_float",
        &xoroshiro["next_float"],
        (0..SAMPLES)
            .map(|_| source.next_float().to_bits() as u64)
            .collect(),
        &mut report,
    );
    let mut source = XoroshiroRandomSource::new(XOROSHIRO_SEED);
    check(
        "xoroshiro.next_gaussian",
        &xoroshiro["next_gaussian"],
        (0..SAMPLES)
            .map(|_| source.next_gaussian().to_bits())
            .collect(),
        &mut report,
    );
    let mut source = XoroshiroRandomSource::new(XOROSHIRO_SEED);
    check(
        "xoroshiro.next_boolean",
        &xoroshiro["next_boolean"],
        (0..SAMPLES)
            .map(|_| u64::from(source.next_boolean()))
            .collect(),
        &mut report,
    );

    // ---- positional factories ---------------------------------------------
    let legacy_factory = LegacyPositionalRandomFactory::new(0x0BAD_F00D_CAFE_BABE);
    let legacy_positional = &fixture["positional_legacy"];
    check(
        "legacy_factory.at",
        &legacy_positional["at"],
        (0..SAMPLES)
            .map(|i| {
                legacy_factory
                    .at(pos_x(i), pos_y(i), pos_z(i))
                    .next_double()
                    .to_bits()
            })
            .collect(),
        &mut report,
    );
    check(
        "legacy_factory.from_hash_of",
        &legacy_positional["from_hash_of"],
        (0..SAMPLES)
            .map(|i| {
                legacy_factory
                    .from_hash_of(&format!("minecraft:feature_{i}"))
                    .next_double()
                    .to_bits()
            })
            .collect(),
        &mut report,
    );
    check(
        "legacy_factory.from_seed",
        &legacy_positional["from_seed"],
        (0..SAMPLES)
            .map(|i| {
                legacy_factory
                    .from_seed(123_456_789 + i as i64)
                    .next_double()
                    .to_bits()
            })
            .collect(),
        &mut report,
    );

    let xoroshiro_factory =
        XoroshiroPositionalRandomFactory::new(0x0123_4567_89AB_CDEF, 0x0FED_CBA9_8765_4321);
    let positional_xoroshiro = &fixture["positional_xoroshiro"];
    check(
        "xoroshiro_factory.at",
        &positional_xoroshiro["at"],
        (0..SAMPLES)
            .map(|i| {
                xoroshiro_factory
                    .at(pos_x(i), pos_y(i), pos_z(i))
                    .next_double()
                    .to_bits()
            })
            .collect(),
        &mut report,
    );
    check(
        "xoroshiro_factory.from_hash_of",
        &positional_xoroshiro["from_hash_of"],
        (0..SAMPLES)
            .map(|i| {
                xoroshiro_factory
                    .from_hash_of(&format!("minecraft:feature_{i}"))
                    .next_double()
                    .to_bits()
            })
            .collect(),
        &mut report,
    );
    check(
        "xoroshiro_factory.from_seed",
        &positional_xoroshiro["from_seed"],
        (0..SAMPLES)
            .map(|i| {
                xoroshiro_factory
                    .from_seed(123_456_789 + i as i64)
                    .next_double()
                    .to_bits()
            })
            .collect(),
        &mut report,
    );

    // ---- RandomSupport ----------------------------------------------------
    let support = &fixture["random_support"];
    let unmixed = RandomSupport::upgrade_seed_to_128bit_unmixed(LEGACY_SEED);
    let mixed = RandomSupport::upgrade_seed_to_128bit(LEGACY_SEED);
    let hashed = RandomSupport::seed_from_hash_of("minecraft:overworld");
    check(
        "random_support.upgrade_unmixed_lo",
        &support["upgrade_unmixed_lo"],
        vec![unmixed.seed_lo as u64],
        &mut report,
    );
    check(
        "random_support.upgrade_unmixed_hi",
        &support["upgrade_unmixed_hi"],
        vec![unmixed.seed_hi as u64],
        &mut report,
    );
    check(
        "random_support.upgrade_lo",
        &support["upgrade_lo"],
        vec![mixed.seed_lo as u64],
        &mut report,
    );
    check(
        "random_support.upgrade_hi",
        &support["upgrade_hi"],
        vec![mixed.seed_hi as u64],
        &mut report,
    );
    check(
        "random_support.hash_lo",
        &support["hash_lo"],
        vec![hashed.seed_lo as u64],
        &mut report,
    );
    check(
        "random_support.hash_hi",
        &support["hash_hi"],
        vec![hashed.seed_hi as u64],
        &mut report,
    );
    check(
        "random_support.mix_stafford_13",
        &support["mix_stafford_13"],
        vec![RandomSupport::mix_stafford_13(LEGACY_SEED) as u64],
        &mut report,
    );

    // ---- noise primitives -------------------------------------------------
    let perlin = PerlinNoise::new(&mut LegacyRandomSource::new(1234));
    check(
        "perlin.get",
        &fixture["perlin"]["samples"],
        (0..SAMPLES)
            .map(|i| {
                let (x, y, z) = noise_coords(i);
                f64::from(perlin.get(x, y, z)).to_bits()
            })
            .collect(),
        &mut report,
    );

    let smeared = SmearedPerlinNoise::new(&mut LegacyRandomSource::new(5678), 1.5);
    check(
        "smeared_perlin.get",
        &fixture["smeared_perlin"]["samples"],
        (0..SAMPLES)
            .map(|i| {
                let (x, y, z) = noise_coords(i);
                f64::from(smeared.get(x, y, z)).to_bits()
            })
            .collect(),
        &mut report,
    );

    let simplex = SimplexNoise::new(&mut LegacyRandomSource::new(9012));
    check(
        "simplex.get_2d",
        &fixture["simplex"]["samples_2d"],
        (0..SAMPLES)
            .map(|i| {
                let (x, _, z) = noise_coords(i);
                f64::from(simplex.get_2d(x, z)).to_bits()
            })
            .collect(),
        &mut report,
    );
    check(
        "simplex.get_3d",
        &fixture["simplex"]["samples_3d"],
        (0..SAMPLES)
            .map(|i| {
                let (x, y, z) = noise_coords(i);
                f64::from(simplex.get(x, y, z)).to_bits()
            })
            .collect(),
        &mut report,
    );

    let normal = NormalNoise::builder()
        .set_base_amplitude(1.0)
        .set_base_octave(-8)
        .set_octave_count(5)
        .set_normalize(true)
        .set_amplitude_modifier(1, 0.5)
        .set_amplitude_modifier(3, 0.0)
        .build();
    let mut random = XoroshiroRandomSource::new(3456);
    let normal_noise = normal.create(&mut random);
    check(
        "normal_noise",
        &fixture["normal_noise"]["samples"],
        (0..SAMPLES)
            .map(|i| {
                let (x, y, z) = noise_coords(i);
                f64::from(normal_noise.get(x, y, z)).to_bits()
            })
            .collect(),
        &mut report,
    );

    let legacy_normal = NormalNoise::builder()
        .set_base_amplitude(1.0)
        .set_base_octave(-6)
        .set_octave_count(4)
        .set_legacy_normalization()
        .build();
    let mut random = LegacyRandomSource::new(7890);
    let legacy_normal_noise = legacy_normal.create_for_legacy_nether_biome(&mut random);
    check(
        "legacy_normal_noise",
        &fixture["legacy_normal_noise"]["samples"],
        (0..SAMPLES)
            .map(|i| {
                let (x, y, z) = noise_coords(i);
                f64::from(legacy_normal_noise.get(x, y, z)).to_bits()
            })
            .collect(),
        &mut report,
    );

    let blended = BlendedNoise::new(1.0, 1.0, 80.0, 160.0, 8.0);
    let mut random = XoroshiroRandomSource::new(2468);
    let sampler = blended.compile_sampler(&mut random);
    check(
        "blended_noise",
        &fixture["blended_noise"]["samples"],
        (0..SAMPLES)
            .map(|i| {
                u64::from(
                    sampler
                        .sample_value(blended_x(i), blended_y(i), blended_z(i))
                        .to_bits(),
                )
            })
            .collect(),
        &mut report,
    );

    let total: usize = report.iter().map(|(_, count)| count).sum();
    println!(
        "ferrite-noise: {} checks matched bit-for-bit ({total} values compared; noise primitives use {SAMPLES} samples each)",
        report.len()
    );
    for (name, count) in &report {
        println!("  {name}: {count}");
    }
}

// ---- comparison helpers ---------------------------------------------------

fn compare_bits(label: &str, expected: &Value, actual: Vec<u64>) -> usize {
    let expected = expected
        .as_array()
        .unwrap_or_else(|| panic!("{label}: fixture is not an array"));
    assert_eq!(
        expected.len(),
        actual.len(),
        "{label}: expected {} values, got {}",
        expected.len(),
        actual.len()
    );
    for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
        let expected = parse_bits(label, expected);
        assert_eq!(
            expected,
            actual,
            "{label}: mismatch at sample {index}: expected {expected:#018x}, got {actual:#018x} \
             ({:?} vs {:?})",
            f64::from_bits(expected),
            f64::from_bits(actual)
        );
    }
    expected.len()
}

fn parse_bits(label: &str, value: &Value) -> u64 {
    if let Some(text) = value.as_str() {
        u64::from_str_radix(text, 16)
            .unwrap_or_else(|error| panic!("{label}: invalid hex {text:?}: {error}"))
    } else if let Some(number) = value.as_i64() {
        number as u32 as u64
    } else if let Some(boolean) = value.as_bool() {
        u64::from(boolean)
    } else {
        panic!("{label}: expected a hex string, integer or boolean, got {value}")
    }
}

// ---- deterministic coordinates (must mirror NoiseReference.java) ----------

fn noise_coords(index: usize) -> (f64, f64, f64) {
    let mut x = (index % 4096) as f64 * 0.3125 - 640.0;
    let y = (index % 211) as f64 * 0.25 - 26.0;
    let z = (index / 4096) as f64 * 0.5 - 12.0;
    let y = if index.is_multiple_of(1000) {
        x = 25_165_824.0 + 1.5;
        -25_165_824.0 - 2.5
    } else {
        y
    };
    (x, y, z)
}

fn pos_x(index: usize) -> i32 {
    (index as i32 * 13) % 2000 - 1000
}

fn pos_y(index: usize) -> i32 {
    (index as i32 * 7) % 128 - 64
}

fn pos_z(index: usize) -> i32 {
    (index as i32 * 29) % 3000 - 1500
}

fn blended_x(index: usize) -> i32 {
    (index as i32 * 37) % 4000 - 2000
}

fn blended_y(index: usize) -> i32 {
    (index as i32 * 17) % 128 - 64
}

fn blended_z(index: usize) -> i32 {
    (index as i32 * 7) % 3000 - 1500
}

// ---- fixture generation ---------------------------------------------------

fn load_fixture() -> Option<Value> {
    let root = workspace_root();
    let jar = find_server_jar(&root)?;

    let fixture = root.join("target/noise-fixtures/reference-26.3.json");
    let harness = root.join("crates/ferrite-noise/tests/java/NoiseReference.java");
    if needs_rebuild(&fixture, &[&harness, &jar]) {
        let Some(java) = find_tool("java") else {
            eprintln!("skipping ferrite-noise reference test: java (JDK 25) not found");
            return None;
        };
        let Some(javac) = find_tool("javac") else {
            eprintln!("skipping ferrite-noise reference test: javac (JDK 25) not found");
            return None;
        };
        if let Err(error) = build_fixture(&root, &jar, &harness, &fixture, &java, &javac) {
            panic!("failed to generate ferrite-noise reference fixture: {error}");
        }
    }

    let text = std::fs::read_to_string(&fixture)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", fixture.display()));
    Some(
        serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("cannot parse {}: {error}", fixture.display())),
    )
}

/// Locates the unobfuscated 26.3 server jar, extracting the nested jar from the
/// vanilla bundler if the convenient copies are missing.
fn find_server_jar(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.join("vanilla/26.3/lib/server-26.3.jar"),
        root.join("vanilla/26.3/versions/26.3/server-26.3.jar"),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let bundle = root.join("vanilla/26.3/server.jar");
    if !bundle.is_file() {
        eprintln!(
            "skipping ferrite-noise reference test: {} not found (run tools/fetch_vanilla.py)",
            bundle.display()
        );
        return None;
    }
    let Some(jar_tool) = find_tool("jar") else {
        eprintln!("skipping ferrite-noise reference test: jar (JDK 25) not found");
        return None;
    };

    let directory = root.join("target/noise-fixtures/bundle");
    if let Err(error) = std::fs::create_dir_all(&directory) {
        eprintln!(
            "skipping ferrite-noise reference test: cannot create {}: {error}",
            directory.display()
        );
        return None;
    }
    let extracted = directory.join("META-INF/versions/26.3/server-26.3.jar");
    let output = Command::new(&jar_tool)
        .arg("--extract")
        .arg("--file")
        .arg(&bundle)
        .arg("--dir")
        .arg(&directory)
        .arg("META-INF/versions/26.3/server-26.3.jar")
        .output();
    match output {
        Ok(output) if output.status.success() && extracted.is_file() => Some(extracted),
        Ok(output) => {
            eprintln!(
                "skipping ferrite-noise reference test: cannot extract the nested server jar: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            None
        }
        Err(error) => {
            eprintln!(
                "skipping ferrite-noise reference test: cannot run {}: {error}",
                jar_tool.display()
            );
            None
        }
    }
}

fn needs_rebuild(fixture: &Path, inputs: &[&Path]) -> bool {
    let Ok(fixture_time) = std::fs::metadata(fixture).and_then(|meta| meta.modified()) else {
        return true;
    };
    inputs.iter().any(|input| {
        std::fs::metadata(input)
            .and_then(|meta| meta.modified())
            .is_ok_and(|time| time > fixture_time)
    })
}

fn build_fixture(
    root: &Path,
    jar: &Path,
    harness: &Path,
    fixture: &Path,
    java: &Path,
    javac: &Path,
) -> Result<(), String> {
    let classes = root.join("target/noise-fixtures/classes");
    std::fs::create_dir_all(&classes).map_err(|error| error.to_string())?;

    let mut classpath = vec![jar.to_path_buf()];
    let libraries = root.join("vanilla/26.3/libraries");
    if libraries.is_dir() {
        collect_jars(&libraries, &mut classpath);
    } else {
        collect_jars(&root.join("vanilla/26.3/lib/libraries"), &mut classpath);
    }
    let separator = if cfg!(windows) { ";" } else { ":" };
    let classpath = classpath
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(separator);

    let output = Command::new(javac)
        .arg("-proc:none")
        .arg("-d")
        .arg(&classes)
        .arg("-cp")
        .arg(&classpath)
        .arg(harness)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", javac.display()))?;
    if !output.status.success() {
        return Err(format!(
            "javac failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let runtime_classpath = format!("{}{separator}{classpath}", classes.display());
    // Pin Math.log to StrictMath/fdlibm: the default libm intrinsic is allowed
    // to differ by one ulp, and that would leak into nextGaussian() fixtures.
    let output = Command::new(java)
        .args(["-XX:+UnlockDiagnosticVMOptions", "-XX:-UseLibmIntrinsic"])
        .arg("-cp")
        .arg(runtime_classpath)
        .arg("NoiseReference")
        .arg(fixture)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", java.display()))?;
    if !output.status.success() {
        return Err(format!(
            "NoiseReference failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn collect_jars(directory: &Path, jars: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jars(&path, jars);
        } else if path.extension().is_some_and(|extension| extension == "jar") {
            jars.push(path);
        }
    }
}

fn find_tool(name: &str) -> Option<PathBuf> {
    let executable = format!("{name}{}", std::env::consts::EXE_SUFFIX);

    // The workspace pins Minecraft 26.3, whose classes need Java 25; prefer the
    // documented JDK before a possibly older machine-wide JAVA_HOME.
    #[cfg(windows)]
    {
        let known = Path::new(r"C:\Program Files\Eclipse Adoptium\jdk-25.0.5.8-hotspot\bin")
            .join(&executable);
        if known.is_file() {
            return Some(known);
        }
    }

    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let candidate = Path::new(&home).join("bin").join(&executable);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let probe = Command::new(&executable).arg("-version").output();
    if probe.is_ok_and(|output| output.status.success()) {
        return Some(PathBuf::from(executable));
    }
    None
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate lives under <workspace>/crates")
        .to_path_buf()
}
