//! Differential test against the real vanilla overworld `MultiNoiseBiomeSource`.
//!
//! The Java harness is compiled and run on demand. If Java 25 or the extracted server jar is
//! unavailable, the test skips with an explanatory message.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrite_worldgen::{ClimateTargetPoint, FeatureGenerator, MultiNoiseBiomeSource, WorldgenData};

const SAMPLE_COUNT: usize = 20_000;
const DEFAULT_JAVA_HOME: &str = "C:/Program Files/Eclipse Adoptium/jdk-25.0.5.8-hotspot";

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn generated_root() -> PathBuf {
    if let Some(root) = std::env::var_os("FERRITE_VANILLA_ROOT") {
        return PathBuf::from(root);
    }
    project_root().join("vanilla/26.3/generated")
}

fn java_home() -> Option<PathBuf> {
    let executable = if cfg!(windows) { "javac.exe" } else { "javac" };
    let default = PathBuf::from(DEFAULT_JAVA_HOME);
    if default.join("bin").join(executable).is_file() {
        return Some(default);
    }
    let from_env = PathBuf::from(std::env::var_os("JAVA_HOME")?);
    if from_env.join("bin").join(executable).is_file() {
        return Some(from_env);
    }
    None
}

fn classpath(root: &Path) -> Option<(String, String)> {
    let server_jar = root.join("vanilla/26.3/lib/server-26.3.jar");
    if !server_jar.is_file() {
        return None;
    }
    let libraries = root.join("vanilla/26.3/lib/libraries");
    if !libraries.is_dir() {
        return None;
    }
    let mut jars = vec![server_jar];
    collect_jars(&libraries, &mut jars);
    if jars.len() < 2 {
        return None;
    }
    let separator = if cfg!(windows) { ";" } else { ":" };
    let joined = jars
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join(separator);
    Some((separator.to_owned(), joined))
}

fn collect_jars(directory: &Path, jars: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jars(&path, jars);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("jar") {
            jars.push(path);
        }
    }
}

fn java_output_dir() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| project_root().join("target"));
    target.join("ferrite-worldgen-java")
}

fn run_java_harness() -> Option<PathBuf> {
    let root = project_root();
    let home = java_home().or_else(|| {
        eprintln!("skipping Java differential test: Java 25 javac not found");
        None
    })?;
    let (separator, dependencies) = classpath(&root).or_else(|| {
        eprintln!(
            "skipping Java differential test: vanilla/26.3/lib/server-26.3.jar or libraries missing"
        );
        None
    })?;

    let output_dir = java_output_dir();
    let classes = output_dir.join("classes");
    fs::create_dir_all(&classes).expect("create Java harness output directory");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/java/BiomeSourceDump.java");
    let javac = home
        .join("bin")
        .join(if cfg!(windows) { "javac.exe" } else { "javac" });
    let java = home
        .join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" });

    let compile = Command::new(javac)
        .arg("-encoding")
        .arg("UTF-8")
        .arg("-cp")
        .arg(&dependencies)
        .arg("-d")
        .arg(&classes)
        .arg(&source)
        .output()
        .expect("run javac");
    if !compile.status.success() {
        panic!(
            "javac failed:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
    }

    let dump = output_dir.join("biome_source_dump.tsv");
    let run_classpath = format!("{}{}{}", classes.display(), separator, dependencies);
    let execution = Command::new(java)
        // Bootstrap writes `logs/latest.log` relative to the working directory.
        .current_dir(&output_dir)
        .arg("-cp")
        .arg(run_classpath)
        .arg("BiomeSourceDump")
        .arg(&dump)
        .output()
        .expect("run Java differential harness");
    if !execution.status.success() {
        panic!(
            "Java differential harness failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&execution.stdout),
            String::from_utf8_lossy(&execution.stderr)
        );
    }
    Some(dump)
}

#[test]
fn matches_vanilla_overworld_multi_noise_source_on_20k_samples() {
    let Some(dump) = run_java_harness() else {
        return;
    };
    let data = WorldgenData::load(generated_root()).expect("load vanilla worldgen data");
    let source = MultiNoiseBiomeSource::from_worldgen(&data, "minecraft:overworld")
        .expect("overworld preset");
    let text = fs::read_to_string(&dump).expect("read Java differential dump");

    let mut samples = 0usize;
    let mut rtree_differences = 0usize;
    for (line_number, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            8,
            "bad Java dump line {}: {line}",
            line_number + 1
        );
        let parse = |index: usize| -> i64 {
            fields[index]
                .parse()
                .unwrap_or_else(|_| panic!("bad number in line {}: {line}", line_number + 1))
        };
        let target =
            ClimateTargetPoint::new(parse(0), parse(1), parse(2), parse(3), parse(4), parse(5));
        let rtree = fields[6];
        let brute_force = fields[7];
        let accelerated = source.find_biome(target).to_string();
        let brute_force_rust = source.find_biome_brute_force(target).to_string();

        assert_eq!(
            brute_force_rust,
            brute_force,
            "brute force disagreed with vanilla at line {}: {target:?}",
            line_number + 1
        );
        assert_eq!(
            accelerated,
            brute_force,
            "accelerated search disagreed with vanilla brute force at line {}: {target:?}",
            line_number + 1
        );
        if rtree != brute_force {
            rtree_differences += 1;
        }
        samples += 1;
    }

    assert_eq!(samples, SAMPLE_COUNT, "unexpected Java sample count");
    println!(
        "Java differential: {samples}/{SAMPLE_COUNT} samples matched vanilla brute force \
         ({rtree_differences} cases where vanilla's hinted R-tree picked a different tied biome)"
    );
}

#[test]
fn density_router_matches_vanilla() {
    let root = project_root();
    let home = java_home().expect("Java 25 required for density differential");
    let (separator, dependencies) = classpath(&root).expect("vanilla jar and libraries required");
    let output_dir = java_output_dir();
    let classes = output_dir.join("density-classes");
    fs::create_dir_all(&classes).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/java/DensityRouterDump.java");
    let javac = home
        .join("bin")
        .join(if cfg!(windows) { "javac.exe" } else { "javac" });
    let java = home
        .join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" });
    let compile = Command::new(javac)
        .args(["-encoding", "UTF-8", "-cp"])
        .arg(&dependencies)
        .arg("-d")
        .arg(&classes)
        .arg(source)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "javac: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let dump = output_dir.join("density_router.tsv");
    let cp = format!("{}{}{}", classes.display(), separator, dependencies);
    let run = Command::new(java)
        .current_dir(&output_dir)
        .arg("-cp")
        .arg(cp)
        .arg("DensityRouterDump")
        .arg(&dump)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "Java: {}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let data = WorldgenData::load(generated_root()).unwrap();
    let settings = data
        .noise_settings
        .get(&"minecraft:overworld".parse().unwrap())
        .unwrap();
    let router = settings.noise_router.resolve_references(&data).unwrap();
    let functions = [
        &router.final_density,
        &router.continents,
        &router.erosion,
        &router.depth,
        &router.ridges,
        &router.temperature,
        &router.vegetation,
        &router.chunk_surface_level,
    ];
    let seeded = ferrite_worldgen::SeededNoise::new(&data, 123456789);
    let mut max_error = 0.0_f64;
    let contents = fs::read_to_string(dump).unwrap();
    let mut count = 0;
    for line in contents.lines() {
        let values: Vec<f64> = line.split('\t').map(|s| s.parse().unwrap()).collect();
        assert_eq!(values.len(), 11);
        let ctx = seeded.at(values[0] as i32, values[1] as i32, values[2] as i32);
        for (i, function) in functions.iter().enumerate() {
            let got = function.compute(&ctx);
            let expected = values[i + 3];
            let error = (got - expected).abs();
            max_error = max_error.max(error);
            assert!(
                error <= 1e-9,
                "point {:?}, field {i}: Rust {got}, Java {expected}, error {error}",
                &values[..3]
            );
        }
        count += 1;
    }
    assert_eq!(count, 2000);
    println!("density differential: {count} points, maximum error {max_error}");
}

#[test]
fn feature_order_matches_vanilla() {
    let root = project_root();
    let Some(home) = java_home() else {
        eprintln!("skipping Java feature-order differential: Java 25 not found");
        return;
    };
    let Some((separator, dependencies)) = classpath(&root) else {
        eprintln!("skipping Java feature-order differential: server jar missing");
        return;
    };
    let output_dir = java_output_dir();
    let classes = output_dir.join("feature-order-classes");
    fs::create_dir_all(&classes).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/java/FeatureOrderDump.java");
    let executable = |name: &str| {
        home.join("bin").join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        })
    };
    let compile = Command::new(executable("javac"))
        .args(["-encoding", "UTF-8", "-cp"])
        .arg(&dependencies)
        .arg("-d")
        .arg(&classes)
        .arg(source)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "javac: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let dump = output_dir.join("feature_order.tsv");
    let cp = format!("{}{}{}", classes.display(), separator, dependencies);
    let run = Command::new(executable("java"))
        .current_dir(&output_dir)
        .arg("-cp")
        .arg(cp)
        .arg("FeatureOrderDump")
        .arg(&dump)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "java: {}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let data = WorldgenData::load(generated_root()).unwrap();
    let generator = FeatureGenerator::new(&data, -540_760_265, -64, 384);
    let contents = fs::read_to_string(dump).unwrap();
    let mut decoration = None;
    let mut compared = 0;
    let mut seeds = 0;
    for line in contents.lines() {
        if line.starts_with("#chunk\t") {
            decoration = line
                .split('\t')
                .nth(4)
                .and_then(|value| value.parse::<i64>().ok());
        }
        if let Some(rest) = line.strip_prefix("#feature_seed\t") {
            let fields: Vec<&str> = rest.split('\t').collect();
            let step: i32 = fields[0].parse().unwrap();
            let index: i32 = fields[1].parse().unwrap();
            let expected: i64 = fields[2].parse().unwrap();
            let seed = ferrite_worldgen::feature_seed(decoration.unwrap(), index, step);
            let mut random = ferrite_worldgen::WorldgenRandom::new(seed);
            let actual = ferrite_noise::RandomSource::next_long(&mut random);
            assert_eq!(
                actual, expected,
                "feature seed mismatch at step {step} index {index}"
            );
            seeds += 1;
            continue;
        }
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let step: usize = fields[0].parse().unwrap();
        let index: usize = fields[1].parse().unwrap();
        let expected = fields[2];
        let actual = generator
            .step_features(step)
            .get(index)
            .map_or_else(|| "<missing>".to_owned(), ToString::to_string);
        assert_eq!(
            actual, expected,
            "feature order mismatch at step {step} index {index}"
        );
        compared += 1;
    }
    // The 26.3 overworld FeatureSorter emits 171 entries across steps 0..10.
    assert_eq!(compared, 171, "unexpected Java feature-order count");
    assert!(seeds > 0, "Java dump had no feature seeds");
    println!(
        "feature order: {compared} entries and {seeds} feature seeds match the 26.3 \
         FeatureSorter"
    );
}

#[test]
fn surface_biome_matches_vanilla() {
    use ferrite_worldgen::{NoiseChunkGenerator, SurfaceGenerator};
    let root = project_root();
    let home = java_home().expect("Java 25 required");
    let (separator, dependencies) = classpath(&root).expect("vanilla jar and libraries required");
    let output_dir = java_output_dir();
    let classes = output_dir.join("surface-biome-classes");
    fs::create_dir_all(&classes).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/java/SurfaceBiomeDump.java");
    let executable = |name: &str| {
        home.join("bin").join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_owned()
        })
    };
    let compile = Command::new(executable("javac"))
        .args(["-encoding", "UTF-8", "-cp"])
        .arg(&dependencies)
        .arg("-d")
        .arg(&classes)
        .arg(source)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "javac: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let dump = output_dir.join("surface_biomes.tsv");
    let cp = format!("{}{}{}", classes.display(), separator, dependencies);
    let run = Command::new(executable("java"))
        .current_dir(&output_dir)
        .arg("-cp")
        .arg(cp)
        .arg("SurfaceBiomeDump")
        .arg(&dump)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "java: {} {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let data = WorldgenData::load(generated_root()).unwrap();
    let noise = NoiseChunkGenerator::new(&data, -540760265).unwrap();
    let surface = SurfaceGenerator::new(&data, &noise, -540760265).unwrap();
    let contents = fs::read_to_string(dump).unwrap();
    let mut count = 0;
    for line in contents.lines() {
        let p: Vec<_> = line.split('\t').collect();
        let x = p[0].parse().unwrap();
        let y = p[1].parse().unwrap();
        let z = p[2].parse().unwrap();
        let expected_quart = (
            p[4].parse().unwrap(),
            p[5].parse().unwrap(),
            p[6].parse().unwrap(),
        );
        assert_eq!(
            surface.biome_quart_at(x, y, z),
            expected_quart,
            "quart for ({x}, {y}, {z})"
        );
        let actual = surface.biome_at(x, y, z).to_string();
        assert_eq!(actual, p[3], "position ({x}, {y}, {z})");
        count += 1;
    }
    assert_eq!(count, 2000);
}
