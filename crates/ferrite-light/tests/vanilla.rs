use ferrite_light::{LightBlockView, LightEngine, LightKind};
use ferrite_nbt::{
    Compound, List, Tag,
    binary::{self, Compression},
};
use std::{collections::HashMap, fs, path::Path};

#[derive(Default)]
struct Chunk {
    sections: HashMap<i32, Section>,
    ready: bool,
}
struct Section {
    states: Vec<u16>,
    sky: Option<Vec<i8>>,
    block: Option<Vec<i8>>,
}
struct Fixture {
    chunks: HashMap<(i32, i32), Chunk>,
}
impl LightBlockView for Fixture {
    fn state(&self, x: i32, y: i32, z: i32) -> u16 {
        self.chunks
            .get(&(x.div_euclid(16), z.div_euclid(16)))
            .and_then(|c| c.sections.get(&y.div_euclid(16)))
            .map_or(0, |s| {
                s.states[(y.rem_euclid(16) as usize) * 256
                    + (z.rem_euclid(16) as usize) * 16
                    + x.rem_euclid(16) as usize]
            })
    }
    fn is_chunk_loaded(&self, x: i32, z: i32) -> bool {
        self.chunks.contains_key(&(x, z))
    }
}
fn compound(tag: &Tag) -> &Compound {
    if let Tag::Compound(v) = tag {
        v
    } else {
        panic!("expected compound")
    }
}
fn read_root(bytes: &[u8], offset: usize) -> Tag {
    let pos = offset * 4096;
    let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
    let compression = match bytes[pos + 4] {
        1 => Compression::Gzip,
        2 => Compression::Zlib,
        3 => Compression::None,
        n => panic!("unsupported compression {n}"),
    };
    binary::read_compressed(&bytes[pos + 5..pos + 4 + len], compression)
        .unwrap()
        .1
}
fn parse_chunk(bytes: &[u8], offset: usize) -> Chunk {
    let root = read_root(bytes, offset);
    let root = compound(&root);
    let Some(Tag::List(List::Compound(sections))) = root.get("sections") else {
        panic!("missing sections")
    };
    let mut out = Chunk {
        ready: matches!(root.get("isLightOn"), Some(Tag::Byte(1)))
            && matches!(root.get("Status"), Some(Tag::String(s)) if s=="minecraft:full"),
        ..Chunk::default()
    };
    for section in sections {
        let Some(Tag::Byte(y)) = section.get("Y") else {
            continue;
        };
        let mut states = vec![0u16; 4096];
        if let Some(Tag::Compound(blocks)) = section.get("block_states") {
            let palette: Vec<u16> = match blocks.get("palette") {
                Some(Tag::List(List::String(names))) => names
                    .iter()
                    .map(|name| {
                        ferrite_data::blocks::default_state(name)
                            .unwrap_or_else(|| panic!("unknown {name}"))
                    })
                    .collect(),
                Some(Tag::List(List::Compound(entries))) => entries
                    .iter()
                    .map(|entry| {
                        let Some(Tag::String(name)) = entry.get("id").or_else(|| entry.get(""))
                        else {
                            panic!("missing block name {entry:?}")
                        };
                        let props: Vec<_> = match entry.get("properties") {
                            Some(Tag::Compound(p)) => p
                                .iter()
                                .map(|(k, v)| match v {
                                    Tag::String(v) => (k.as_str(), v.as_str()),
                                    _ => panic!("invalid property"),
                                })
                                .collect(),
                            _ => Vec::new(),
                        };
                        ferrite_data::blocks::state_id(name, &props)
                            .unwrap_or_else(|| panic!("unknown {name} {props:?}"))
                    })
                    .collect(),
                _ => panic!("missing palette"),
            };
            if palette.len() == 1 {
                states.fill(palette[0]);
            } else {
                let Some(Tag::LongArray(data)) = blocks.get("data") else {
                    panic!("missing packed states")
                };
                let bits = (usize::BITS - (palette.len() - 1).leading_zeros()).max(4) as usize;
                let per = 64 / bits;
                let mask = (1u64 << bits) - 1;
                for (i, state) in states.iter_mut().enumerate() {
                    *state =
                        palette[((data[i / per] as u64 >> ((i % per) * bits)) & mask) as usize];
                }
            }
        }
        let byte_array = |name| match section.get(name) {
            Some(Tag::ByteArray(v)) => Some(v.clone()),
            _ => None,
        };
        out.sections.insert(
            i32::from(*y),
            Section {
                states,
                sky: byte_array("SkyLight"),
                block: byte_array("BlockLight"),
            },
        );
    }
    out
}
#[test]
fn compare_fixture_light_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../vanilla/26.3/fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca",
    );
    if !path.exists() {
        eprintln!("SKIP vanilla light differential: {} absent", path.display());
        return;
    }
    let bytes = fs::read(&path).expect("read fixture region");
    assert!(bytes.len() >= 8192, "truncated region header");
    let location = |x: usize, z: usize| -> usize {
        let i = (x + z * 32) * 4;
        ((bytes[i] as usize) << 16) | ((bytes[i + 1] as usize) << 8) | bytes[i + 2] as usize
    };
    let mut ready = std::collections::HashSet::new();
    for z in 0..32 {
        for x in 0..32 {
            let offset = location(x, z);
            if offset == 0 {
                continue;
            }
            let root = read_root(&bytes, offset);
            let root = compound(&root);
            if matches!(root.get("isLightOn"), Some(Tag::Byte(1)))
                && matches!(root.get("Status"),Some(Tag::String(s)) if s=="minecraft:full")
            {
                ready.insert((x, z));
            }
        }
    }
    let candidates: Vec<_> = (1..31)
        .flat_map(|z| (1..31).map(move |x| (x, z)))
        .filter(|&(x, z)| {
            (z - 1..=z + 1).all(|nz| (x - 1..=x + 1).all(|nx| ready.contains(&(nx, nz))))
        })
        .collect();
    eprintln!(
        "vanilla light differential: {} comparable chunk(s), {} full lit chunks in region",
        candidates.len(),
        ready.len()
    );
    if candidates.is_empty() {
        eprintln!("SKIP vanilla light differential: no fully lit 3x3 neighborhood");
        return;
    }
    let mut total_mismatch = [0usize; 2];
    let mut total_compared = [0usize; 2];
    for (cx, cz) in candidates {
        let mut chunks = HashMap::new();
        for z in cz - 1..=cz + 1 {
            for x in cx - 1..=cx + 1 {
                chunks.insert((x as i32, z as i32), parse_chunk(&bytes, location(x, z)));
            }
        }
        if chunks.values().any(|chunk| !chunk.ready) {
            eprintln!("SKIP chunk {cx},{cz}: neighbor lacks initialized light");
            continue;
        }
        let world = Fixture { chunks };
        let mut engine = LightEngine::new(cx as i32, cz as i32, -4, 19, true);
        engine.relight(&world);
        let mut mismatch = [0usize; 2];
        let mut compared = [0usize; 2];
        for (&sy, section) in &world.chunks[&(cx as i32, cz as i32)].sections {
            for (kind, array, slot) in [
                (LightKind::Block, &section.block, 0),
                (LightKind::Sky, &section.sky, 1),
            ] {
                let Some(array) = array else {
                    continue;
                };
                assert_eq!(array.len(), 2048);
                for i in 0..4096 {
                    let x = (cx as i32) * 16 + (i % 16) as i32;
                    let z = (cz as i32) * 16 + ((i / 16) % 16) as i32;
                    let y = sy * 16 + (i / 256) as i32;
                    if let Some(actual) = engine.get(kind, x, y, z) {
                        let expected = ((array[i / 2] as u8) >> ((i % 2) * 4)) & 15;
                        compared[slot] += 1;
                        if actual != expected && total_mismatch[slot] + mismatch[slot] < 20 {
                            eprintln!(
                                "{kind:?} mismatch {x} {y} {z}: actual {actual} expected {expected} state {:?}",
                                ferrite_data::blocks::state(world.state(x, y, z))
                            );
                        }
                        mismatch[slot] += usize::from(actual != expected);
                    }
                }
            }
        }
        for slot in 0..2 {
            total_mismatch[slot] += mismatch[slot];
            total_compared[slot] += compared[slot];
        }
    }
    eprintln!(
        "vanilla differential total: block {}/{} ({:.5}%), sky {}/{} ({:.5}%)",
        total_mismatch[0],
        total_compared[0],
        total_mismatch[0] as f64 * 100.0 / total_compared[0] as f64,
        total_mismatch[1],
        total_compared[1],
        total_mismatch[1] as f64 * 100.0 / total_compared[1] as f64
    );
    assert!(
        total_compared[0] > 0 && total_compared[1] > 0,
        "fixture has no stored light arrays"
    );
    assert_eq!(total_mismatch, [0, 0], "vanilla light mismatch");
}
