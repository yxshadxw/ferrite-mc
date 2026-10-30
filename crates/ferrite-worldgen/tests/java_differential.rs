//! Differential test against the real vanilla overworld `MultiNoiseBiomeSource`.
//!
//! The Java harness is compiled and run on demand. If Java 25 or the extracted server jar is
//! unavailable, the test skips with an explanatory message.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrite_worldgen::{ClimateTargetPoint, MultiNoiseBiomeSource, WorldgenData};

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
    let javac = home.join("bin").join(if cfg!(windows) { "javac.exe" } else { "javac" });
    let java = home.join("bin").join(if cfg!(windows) { "java.exe" } else { "java" });
    let compile = Command::new(javac).args(["-encoding", "UTF-8", "-cp"]).arg(&dependencies).arg("-d").arg(&classes).arg(source).output().unwrap();
    assert!(compile.status.success(), "javac: {}", String::from_utf8_lossy(&compile.stderr));
    let dump = output_dir.join("density_router.tsv");
    let cp = format!("{}{}{}", classes.display(), separator, dependencies);
    let run = Command::new(java).current_dir(&output_dir).arg("-cp").arg(cp).arg("DensityRouterDump").arg(&dump).output().unwrap();
    assert!(run.status.success(), "Java: {}\n{}", String::from_utf8_lossy(&run.stdout), String::from_utf8_lossy(&run.stderr));
    let data = WorldgenData::load(generated_root()).unwrap();
    let settings = data.noise_settings.get(&"minecraft:overworld".parse().unwrap()).unwrap();
    let router = settings.noise_router.resolve_references(&data).unwrap();
    let functions = [&router.final_density, &router.continents, &router.erosion, &router.depth, &router.ridges, &router.temperature, &router.vegetation, &router.chunk_surface_level];
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
            let expected = values[i+3];
            let error = (got - expected).abs();
            max_error = max_error.max(error);
            assert!(error <= 1e-9, "point {:?}, field {i}: Rust {got}, Java {expected}, error {error}", &values[..3]);
        }
        count += 1;
    }
    assert_eq!(count, 2000);
    println!("density differential: {count} points, maximum error {max_error}");
}
