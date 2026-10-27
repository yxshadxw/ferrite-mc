use ferrite_nbt::{Tag, binary};
use ferrite_world::{
    BlockStateRegistry, DATA_VERSION, HeightmapKind, RegionCompression, RegionFile,
    RuntimeRegistry, chunk_from_nbt, chunk_to_nbt,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicIsize, Ordering},
    time::Instant,
};
struct Alloc;
static BYTES: AtomicIsize = AtomicIsize::new(0);
unsafe impl GlobalAlloc for Alloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            BYTES.fetch_add(layout.size() as isize, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        BYTES.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe {
            System.dealloc(p, layout);
        }
    }
    unsafe fn realloc(&self, p: *mut u8, old: Layout, new: usize) -> *mut u8 {
        let result = unsafe { System.realloc(p, old, new) };
        if !result.is_null() {
            BYTES.fetch_add(new as isize - old.size() as isize, Ordering::Relaxed);
        }
        result
    }
}
#[global_allocator]
static ALLOC: Alloc = Alloc;
#[test]
fn vanilla_complete_chunks_match_heightmaps_and_roundtrip() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3");
    let path = base.join("fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca");
    if !path.exists() {
        eprintln!("skipping absent vanilla fixture");
        return;
    }
    let mut jar =
        zip::ZipArchive::new(std::fs::File::open(base.join("server.jar")).unwrap()).unwrap();
    let version: serde_json::Value =
        serde_json::from_reader(jar.by_name("version.json").unwrap()).unwrap();
    assert_eq!(version["world_version"], DATA_VERSION);
    let registry = RuntimeRegistry::load(base.join("generated")).unwrap();
    let bedrock = registry
        .block_id("minecraft:bedrock", &Default::default())
        .unwrap();
    let region = RegionFile::open(path, 0, 0).unwrap();
    let target = std::env::temp_dir().join(format!("ferrite-fixture-{}", std::process::id()));
    std::fs::create_dir_all(&target).unwrap();
    let out = RegionFile::open(target.join("r.0.0.mca"), 0, 0).unwrap();
    let mut count = 0;
    let mut incomplete = 0;
    let mut load_time = std::time::Duration::ZERO;
    let mut memory = 0isize;
    for z in 0..32 {
        for x in 0..32 {
            let Some(bytes) = region.read_chunk(x, z).unwrap() else {
                continue;
            };
            let (_, tag) = binary::read_named(&bytes).unwrap();
            let Tag::Compound(root) = &tag else {
                panic!("root")
            };
            if root.get("Status") != Some(&Tag::String("minecraft:full".into())) {
                incomplete += 1;
                continue;
            }
            let start_mem = BYTES.load(Ordering::Relaxed);
            let start = Instant::now();
            let data = region.read_chunk(x, z).unwrap().unwrap();
            let (_, input) = binary::read_named(&data).unwrap();
            let chunk = chunk_from_nbt(&input, -64, 384, &registry).unwrap();
            drop(input);
            drop(data);
            load_time += start.elapsed();
            memory +=
                BYTES.load(Ordering::Relaxed) - start_mem + std::mem::size_of_val(&chunk) as isize;
            for zz in 0..16 {
                for xx in 0..16 {
                    assert_eq!(
                        chunk.block(xx, -64, zz),
                        Some(bedrock),
                        "chunk {x},{z} at {xx},{zz}"
                    );
                }
            }
            let Tag::Compound(heights) = root.get("Heightmaps").unwrap() else {
                panic!("heightmaps")
            };
            for (name, kind) in [
                ("WORLD_SURFACE", HeightmapKind::WorldSurface),
                ("OCEAN_FLOOR", HeightmapKind::OceanFloor),
                ("MOTION_BLOCKING", HeightmapKind::MotionBlocking),
                (
                    "MOTION_BLOCKING_NO_LEAVES",
                    HeightmapKind::MotionBlockingNoLeaves,
                ),
            ] {
                let Tag::LongArray(words) = heights.get(name).unwrap() else {
                    panic!("{name}")
                };
                for i in 0..256 {
                    assert_eq!(
                        chunk.heights(kind)[i],
                        ((words[i / 7] as u64 >> ((i % 7) * 9)) & 511) as u16,
                        "chunk {x},{z} {name} col {i}"
                    );
                }
            }
            let encoded = chunk_to_nbt(&chunk, &registry).unwrap();
            let written = binary::write_named("", &encoded).unwrap();
            out.write_chunk(x, z, &written, RegionCompression::Zlib)
                .unwrap();
            let restored_data = out.read_chunk(x, z).unwrap().unwrap();
            let (_, restored_tag) = binary::read_named(&restored_data).unwrap();
            let restored = chunk_from_nbt(&restored_tag, -64, 384, &registry).unwrap();
            for yy in -64..320 {
                for zz in 0..16 {
                    for xx in 0..16 {
                        assert_eq!(chunk.block(xx, yy, zz), restored.block(xx, yy, zz));
                    }
                }
            }
            count += 1;
        }
    }
    assert!(count > 0);
    eprintln!(
        "fixture: {count} full, {incomplete} incomplete; mean retained chunk {} bytes incl struct; mean read/decompress/parse/convert {:?}",
        memory / count as isize,
        load_time / count as u32
    );
    drop(out);
    std::fs::remove_dir_all(target).unwrap();
}
