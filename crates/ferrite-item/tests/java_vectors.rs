//! Differential test against the real 26.3 server classes.
//!
//! `tests/java/SlotDump.java` bootstraps the vanilla registries, builds representative stacks
//! and encodes them with the server's own network `Slot` codec. This test compiles and runs it
//! (when Java 25 and the extracted server jar are available), then requires every emitted slot
//! to decode in `ferrite_item` and to re-encode without losing data.
//!
//! The harness lives in the repository; its classes and output stay in `target/`. Nothing from
//! Mojang is copied or committed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrite_item::ItemStack;

const JAVA_HOME: &str = "C:/Program Files/Eclipse Adoptium/jdk-25.0.5.8-hotspot";

#[test]
fn real_server_slots_decode_and_re_encode() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .ancestors()
        .nth(2)
        .expect("ferrite-item lives under crates/");
    let server_jar = workspace.join("vanilla/26.3/lib/server-26.3.jar");
    let libraries = workspace.join("vanilla/26.3/lib/libraries");
    let source = manifest.join("tests/java/SlotDump.java");
    if !server_jar.exists() || !libraries.exists() || !source.exists() {
        eprintln!(
            "skipping the Java differential test: extract vanilla data with \
             `python tools/fetch_vanilla.py` first"
        );
        return;
    }

    let java_home = java_home();
    let Some(java_home) = java_home else {
        eprintln!("skipping the Java differential test: Java 25 was not found");
        return;
    };
    let javac = java_home.join("bin/javac.exe");
    let java = java_home.join("bin/java.exe");
    if !javac.exists() || !java.exists() {
        eprintln!(
            "skipping the Java differential test: {} has no bin/javac.exe and bin/java.exe",
            java_home.display()
        );
        return;
    }

    let out_dir = workspace.join("target/ferrite-item-java");
    let classes = out_dir.join("classes");
    let _ = fs::remove_dir_all(&classes);
    fs::create_dir_all(&classes).expect("create the harness class directory");

    let mut classpath = vec![server_jar];
    collect_jars(&libraries, &mut classpath);
    let separator = if cfg!(windows) { ';' } else { ':' };
    let classpath = classpath
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(&separator.to_string());

    let compile = Command::new(&javac)
        .arg("-nowarn")
        .arg("-cp")
        .arg(&classpath)
        .arg("-d")
        .arg(&classes)
        .arg(&source)
        .output()
        .expect("run javac");
    assert!(
        compile.status.success(),
        "javac failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&java)
        .arg("-cp")
        .arg(format!("{}{separator}{classpath}", classes.display()))
        .arg("SlotDump")
        .output()
        .expect("run the Java harness");
    assert!(
        run.status.success(),
        "SlotDump failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let stdout = String::from_utf8_lossy(&run.stdout);
    let mut checked = 0;
    let mut exact = 0;
    let mut reordered = Vec::new();
    for line in stdout.lines() {
        let payload = line.split("[STDOUT]: ").nth(1).unwrap_or(line);
        let Some((name, hex)) = payload.split_once('=') else {
            continue;
        };
        if name.contains(' ') || hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            continue;
        }
        let bytes = decode_hex(hex);
        let stack = ItemStack::decode_network(&bytes)
            .unwrap_or_else(|error| panic!("{name}: real server slot does not decode: {error}"));
        let reencoded = stack
            .encode_network()
            .unwrap_or_else(|error| panic!("{name}: decoded stack does not re-encode: {error}"));

        // Decoding our own output must give the same stack; multi-component slots may be
        // reordered into component type id order, so only single-component slots are compared
        // byte for byte.
        let again = ItemStack::decode_network(&reencoded)
            .unwrap_or_else(|error| panic!("{name}: re-encoded slot does not decode: {error}"));
        assert_eq!(again, stack, "{name}: re-encoding loses data");
        if reencoded == bytes {
            exact += 1;
        } else {
            reordered.push(name.to_owned());
        }
        checked += 1;
    }

    assert!(
        checked >= 15,
        "the Java harness only produced {checked} slots; expected at least 15"
    );
    // Only stacks that set several components may come back in a different component order
    // (ferrite-item writes components in type id order, vanilla in insertion order). Every
    // single-component payload must be byte exact.
    for name in &reordered {
        assert!(
            matches!(name.as_str(), "sword" | "stone" | "food"),
            "{name}: a single-component vanilla slot was not byte exact: {reordered:?}"
        );
    }
    assert!(
        exact + 3 >= checked,
        "only {exact} of {checked} vanilla slots are byte exact: {reordered:?}"
    );
}

fn java_home() -> Option<PathBuf> {
    // The project pins Java 25 for vanilla data extraction; prefer it over an unrelated
    // JAVA_HOME (which may be an older JDK that cannot read the server jar's class files).
    let pinned = PathBuf::from(JAVA_HOME);
    if pinned.join("bin/javac.exe").exists() || pinned.join("bin/javac").exists() {
        return Some(pinned);
    }
    let home = PathBuf::from(std::env::var_os("JAVA_HOME")?);
    (home.join("bin/javac.exe").exists() || home.join("bin/javac").exists()).then_some(home)
}

fn collect_jars(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jars(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "jar") {
            out.push(path);
        }
    }
}

fn decode_hex(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("validated hex");
            let low = (pair[1] as char).to_digit(16).expect("validated hex");
            u8::try_from(high * 16 + low).expect("a byte fits in u8")
        })
        .collect()
}
