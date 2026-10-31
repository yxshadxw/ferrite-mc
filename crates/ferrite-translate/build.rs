//! Generates the ID mapping tables of every version pair from the vanilla data generator
//! reports of both versions (`vanilla/<version>/generated/`, produced by
//! `python tools/fetch_vanilla.py <version>`).
//!
//! Per pair `newer -> older` this emits `OUT_DIR/<module>.rs` with:
//! - packet names per (state, direction) for both versions and the ID remaps between them
//!   (matched by packet name),
//! - block state remaps in both directions (matched by block name + properties, with the
//!   pair's fallback rules and a closest-state search for states the other version lacks),
//! - remaps for every static registry both versions have (matched by entry name, then the
//!   fallback rules, then the registry's default entry),
//! - the entry names of the data-driven (synchronized) registries the older version ships,
//!   so the translator can drop entries the older client cannot resolve.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// One adjacent version step. Adding a version means adding an entry here, a module under
/// `src/pairs/`, and a `fallbacks.txt` next to it.
struct Pair {
    module: &'static str,
    newer: &'static str,
    older: &'static str,
}

const PAIRS: &[Pair] = &[Pair {
    module: "v777_v776",
    newer: "26.3",
    older: "26.2",
}];

/// Data-driven registries that are synchronized to the client during configuration.
const SYNCED: &[&str] = &[
    "banner_pattern",
    "cat_sound_variant",
    "cat_variant",
    "chat_type",
    "chicken_sound_variant",
    "chicken_variant",
    "cow_sound_variant",
    "cow_variant",
    "damage_type",
    "decorated_pot_pattern",
    "dialog",
    "dimension_type",
    "enchantment",
    "frog_variant",
    "instrument",
    "jukebox_song",
    "painting_variant",
    "pig_sound_variant",
    "pig_variant",
    "sulfur_cube_archetype",
    "test_environment",
    "test_instance",
    "timeline",
    "trim_material",
    "trim_pattern",
    "wolf_sound_variant",
    "wolf_variant",
    "world_clock",
    "worldgen/biome",
    "zombie_nautilus_variant",
];

const STATES: [&str; 5] = ["handshake", "status", "login", "configuration", "play"];
const BOUNDS: [&str; 2] = ["serverbound", "clientbound"];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let vanilla = manifest.join("../../vanilla");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for pair in PAIRS {
        let fallbacks_path = manifest
            .join("src/pairs")
            .join(pair.module)
            .join("fallbacks.txt");
        println!("cargo:rerun-if-changed={}", fallbacks_path.display());
        let fallbacks = parse_fallbacks(
            &fs::read_to_string(&fallbacks_path)
                .unwrap_or_else(|e| panic!("read {}: {e}", fallbacks_path.display())),
        );
        let newer = Version::load(&vanilla, pair.newer);
        // Keep the workspace buildable on a checkout without the older version's
        // gitignored reports: generate identity tables and state what to run. The real
        // remaps appear as soon as `python tools/fetch_vanilla.py <older>` has run.
        let older = if vanilla
            .join(pair.older)
            .join("generated/reports/packets.json")
            .exists()
        {
            Version::load(&vanilla, pair.older)
        } else {
            println!(
                "cargo:warning=missing vanilla {} reports; generating identity tables (run `python tools/fetch_vanilla.py {}` for real translation)",
                pair.older, pair.older
            );
            Version::load(&vanilla, pair.newer)
        };
        let code = generate(&newer, &older, &fallbacks);
        fs::write(out.join(format!("{}.rs", pair.module)), code).unwrap();
    }
}

struct Version {
    packets: Value,
    registries: Value,
    blocks: Value,
    data: PathBuf,
}

impl Version {
    fn load(vanilla: &Path, version: &str) -> Self {
        let generated = vanilla.join(version).join("generated");
        let reports = generated.join("reports");
        for file in ["packets.json", "registries.json", "blocks.json"] {
            println!("cargo:rerun-if-changed={}", reports.join(file).display());
        }
        let read = |file: &str| -> Value {
            let path = reports.join(file);
            let bytes = fs::read(&path).unwrap_or_else(|e| {
                panic!(
                    "missing vanilla report {} ({e}); run `python tools/fetch_vanilla.py {version}`",
                    path.display()
                )
            });
            serde_json::from_slice(&bytes).unwrap()
        };
        Self {
            packets: read("packets.json"),
            registries: read("registries.json"),
            blocks: read("blocks.json"),
            data: generated.join("data/minecraft"),
        }
    }

    /// Packet names indexed by protocol ID for one state and direction.
    fn packet_names(&self, state: &str, bound: &str) -> Vec<String> {
        let Some(map) = self.packets[state][bound].as_object() else {
            return Vec::new();
        };
        let mut names = vec![String::new(); map.len()];
        for (name, v) in map {
            let id = v["protocol_id"].as_u64().unwrap() as usize;
            if id >= names.len() {
                names.resize(id + 1, String::new());
            }
            names[id] = name.clone();
        }
        names
    }

    /// Entries of a static registry indexed by protocol ID, plus its default entry.
    fn registry(&self, name: &str) -> Option<(Vec<String>, Option<String>)> {
        let reg = self.registries.get(name)?;
        let entries = reg["entries"].as_object()?;
        let mut names = vec![String::new(); entries.len()];
        for (entry, v) in entries {
            names[v["protocol_id"].as_u64().unwrap() as usize] = entry.clone();
        }
        Some((names, reg["default"].as_str().map(str::to_owned)))
    }

    /// Entry names of a data-driven registry, or `None` if this version does not have it.
    fn data_registry(&self, name: &str) -> Option<Vec<String>> {
        let dir = self.data.join(name);
        if !dir.is_dir() {
            return None;
        }
        let mut out = Vec::new();
        walk(&dir, &dir, &mut out);
        out.sort();
        Some(out)
    }

    /// Block states indexed by global ID: (block name, properties, is default).
    fn block_states(&self) -> Vec<State> {
        let mut states = Vec::new();
        for (name, block) in self.blocks.as_object().unwrap() {
            for s in block["states"].as_array().unwrap() {
                let id = s["id"].as_u64().unwrap() as usize;
                let props = s["properties"]
                    .as_object()
                    .map(|p| {
                        p.iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                            .collect()
                    })
                    .unwrap_or_default();
                if id >= states.len() {
                    states.resize(id + 1, None);
                }
                states[id] = Some(State {
                    block: name.clone(),
                    props,
                    default: s["default"].as_bool().unwrap_or(false),
                });
            }
        }
        states
            .into_iter()
            .map(|s| s.expect("block state IDs are dense"))
            .collect()
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for item in fs::read_dir(dir).unwrap() {
        let path = item.unwrap().path();
        if path.is_dir() {
            walk(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "json") {
            let rel = path.strip_prefix(root).unwrap().with_extension("");
            out.push(format!(
                "minecraft:{}",
                rel.to_string_lossy().replace('\\', "/")
            ));
        }
    }
}

#[derive(Clone)]
struct State {
    block: String,
    props: BTreeMap<String, String>,
    default: bool,
}

/// `registries pattern -> target`; `*` in the pattern captures one run of characters that
/// is substituted for `*` in the target.
struct Fallback {
    registries: Vec<String>,
    pattern: String,
    target: String,
}

fn parse_fallbacks(text: &str) -> Vec<Fallback> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        let (lhs, target) = line
            .split_once("->")
            .unwrap_or_else(|| panic!("fallbacks.txt:{}: expected `->`", n + 1));
        let mut lhs = lhs.split_whitespace();
        let (Some(registries), Some(pattern), None) = (lhs.next(), lhs.next(), lhs.next()) else {
            panic!(
                "fallbacks.txt:{}: expected `<registries> <pattern> -> <target>`",
                n + 1
            );
        };
        out.push(Fallback {
            registries: registries.split(',').map(str::to_owned).collect(),
            pattern: pattern.to_owned(),
            target: target.trim().to_owned(),
        });
    }
    out
}

/// Same algorithm as `tables::apply_fallback` at runtime.
fn apply_fallback(rules: &[Fallback], registry: &str, name: &str) -> Option<String> {
    let short = registry.strip_prefix("minecraft:").unwrap_or(registry);
    for rule in rules {
        if !rule.registries.iter().any(|r| r == short) {
            continue;
        }
        if let Some((prefix, suffix)) = rule.pattern.split_once('*') {
            if name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
            {
                let capture = &name[prefix.len()..name.len() - suffix.len()];
                return Some(rule.target.replace('*', capture));
            }
        } else if rule.pattern == name {
            return Some(rule.target.clone());
        }
    }
    None
}

/// For every `from` state, the closest `to` state: same block (or its fallback), the most
/// equal property values, ties broken in favour of the target block's default state.
fn map_states(from: &[State], to: &[State], rules: &[Fallback]) -> Vec<u32> {
    let mut by_block: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut exact: HashMap<(&str, &BTreeMap<String, String>), usize> = HashMap::new();
    for (id, s) in to.iter().enumerate() {
        by_block.entry(&s.block).or_default().push(id);
        exact.insert((&s.block, &s.props), id);
    }
    let air = by_block["minecraft:air"][0];
    from.iter()
        .map(|s| {
            if let Some(&id) = exact.get(&(s.block.as_str(), &s.props)) {
                return id as u32;
            }
            let target = if by_block.contains_key(s.block.as_str()) {
                Some(s.block.clone())
            } else {
                apply_fallback(rules, "minecraft:block", &s.block)
            };
            let Some(candidates) = target.as_deref().and_then(|t| by_block.get(t)) else {
                return air as u32;
            };
            let score = |id: usize| {
                let props = &to[id].props;
                let equal = s
                    .props
                    .iter()
                    .filter(|(k, v)| props.get(*k) == Some(v))
                    .count();
                (equal, to[id].default)
            };
            *candidates.iter().max_by_key(|&&id| score(id)).unwrap() as u32
        })
        .collect()
}

fn map_registry(
    registry: &str,
    from: &[String],
    to: &[String],
    rules: Option<&[Fallback]>,
) -> Vec<i32> {
    let index: HashMap<&str, i32> = to
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i as i32))
        .collect();
    from.iter()
        .map(|name| {
            if let Some(&id) = index.get(name.as_str()) {
                return id;
            }
            if let Some(target) = rules.and_then(|r| apply_fallback(r, registry, name))
                && let Some(&id) = index.get(target.as_str())
            {
                return id;
            }
            -1
        })
        .collect()
}

fn default_id(entries: &[String], default: Option<&str>) -> i32 {
    default
        .and_then(|d| entries.iter().position(|e| e == d))
        .map_or(-1, |i| i as i32)
}

fn generate(newer: &Version, older: &Version, rules: &[Fallback]) -> String {
    let mut code =
        String::from("// @generated by build.rs from the vanilla reports; do not edit.\n");

    // Packets.
    let mut new_names = Vec::new();
    let mut old_names = Vec::new();
    for state in STATES {
        for bound in BOUNDS {
            new_names.push(newer.packet_names(state, bound));
            old_names.push(older.packet_names(state, bound));
        }
    }
    let names_table = |tables: &[Vec<String>]| {
        let mut s = String::from("[");
        for t in tables {
            s.push_str("&[");
            for n in t {
                write!(s, "{n:?},").unwrap();
            }
            s.push_str("],");
        }
        s.push(']');
        s
    };
    let remap_table = |from: &[Vec<String>], to: &[Vec<String>]| {
        let mut s = String::from("[");
        for (f, t) in from.iter().zip(to) {
            let index: HashMap<&str, usize> =
                t.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
            s.push_str("&[");
            for n in f {
                write!(s, "{},", index.get(n.as_str()).map_or(-1, |&i| i as i32)).unwrap();
            }
            s.push_str("],");
        }
        s.push(']');
        s
    };
    writeln!(
        code,
        "pub static NEW_PACKETS: [&[&str]; 10] = {};",
        names_table(&new_names)
    )
    .unwrap();
    writeln!(
        code,
        "pub static OLD_PACKETS: [&[&str]; 10] = {};",
        names_table(&old_names)
    )
    .unwrap();
    writeln!(
        code,
        "pub static PACKETS_DOWN: [&[i32]; 10] = {};",
        remap_table(&new_names, &old_names)
    )
    .unwrap();
    writeln!(
        code,
        "pub static PACKETS_UP: [&[i32]; 10] = {};",
        remap_table(&old_names, &new_names)
    )
    .unwrap();

    // Block states.
    let new_states = newer.block_states();
    let old_states = older.block_states();
    let write_u32s = |code: &mut String, name: &str, values: &[u32]| {
        write!(code, "pub static {name}: [u32; {}] = [", values.len()).unwrap();
        for v in values {
            write!(code, "{v},").unwrap();
        }
        code.push_str("];\n");
    };
    write_u32s(
        &mut code,
        "BLOCK_STATES_DOWN",
        &map_states(&new_states, &old_states, rules),
    );
    write_u32s(
        &mut code,
        "BLOCK_STATES_UP",
        &map_states(&old_states, &new_states, &[]),
    );

    // Static registries present in both versions.
    let mut regs = Vec::new();
    for name in newer.registries.as_object().unwrap().keys() {
        let (Some((new_entries, new_default)), Some((old_entries, old_default))) =
            (newer.registry(name), older.registry(name))
        else {
            continue;
        };
        let down = map_registry(name, &new_entries, &old_entries, Some(rules));
        let up = map_registry(name, &old_entries, &new_entries, None);
        let defaults = (
            default_id(&old_entries, old_default.as_deref()),
            default_id(&new_entries, new_default.as_deref()),
        );
        regs.push((name.clone(), down, up, defaults));
    }
    regs.sort_by(|a, b| a.0.cmp(&b.0));
    code.push_str("pub static REGISTRIES: &[crate::tables::RegistryTable] = &[\n");
    for (name, down, up, (down_default, up_default)) in &regs {
        write!(
            code,
            "crate::tables::RegistryTable {{ name: {name:?}, down: &["
        )
        .unwrap();
        for v in down {
            write!(code, "{v},").unwrap();
        }
        code.push_str("], up: &[");
        for v in up {
            write!(code, "{v},").unwrap();
        }
        writeln!(
            code,
            "], down_default: {down_default}, up_default: {up_default} }},"
        )
        .unwrap();
    }
    code.push_str("];\n");

    // Static registries of the older version (to know which tag registries it has).
    let old_static: BTreeSet<&String> = older.registries.as_object().unwrap().keys().collect();
    code.push_str("pub static OLD_STATIC_REGISTRIES: &[&str] = &[");
    for name in &old_static {
        write!(code, "{name:?},").unwrap();
    }
    code.push_str("];\n");

    // Data-driven registries of the older version.
    code.push_str("pub static OLD_DATA_REGISTRIES: &[(&str, &[&str])] = &[\n");
    let mut synced: Vec<&str> = SYNCED.to_vec();
    synced.sort();
    for name in synced {
        if let Some(entries) = older.data_registry(name) {
            write!(code, "(\"minecraft:{name}\", &[").unwrap();
            for e in entries {
                write!(code, "{e:?},").unwrap();
            }
            code.push_str("]),\n");
        }
    }
    code.push_str("];\n");

    // Fallback rules, for data-driven registries resolved at runtime.
    code.push_str("pub static FALLBACKS: &[crate::tables::Fallback] = &[\n");
    for rule in rules {
        for registry in &rule.registries {
            writeln!(
                code,
                "crate::tables::Fallback {{ registry: \"minecraft:{registry}\", pattern: {:?}, target: {:?} }},",
                rule.pattern, rule.target
            )
            .unwrap();
        }
    }
    code.push_str("];\n");
    code
}
