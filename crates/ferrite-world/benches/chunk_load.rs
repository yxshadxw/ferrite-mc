use criterion::{Criterion, criterion_group, criterion_main};
use ferrite_nbt::{Tag, binary};
use ferrite_world::{RegionFile, RuntimeRegistry, chunk_from_nbt};
use std::hint::black_box;

fn chunk_load(c: &mut Criterion) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3");
    let registry = RuntimeRegistry::load(base.join("generated")).unwrap();
    let region = RegionFile::open(
        base.join("fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca"),
        0,
        0,
    )
    .unwrap();
    let input = (0..32).flat_map(|z| (0..32).map(move |x| (x, z))).filter_map(|(x,z)| region.read_chunk(x,z).unwrap()).filter_map(|bytes| binary::read_named(&bytes).ok().map(|(_,tag)| tag)).find(|tag| matches!(tag, Tag::Compound(root) if root.get("Status") == Some(&Tag::String("minecraft:full".into())))).expect("fixture lacks a full chunk");
    c.bench_function("chunk_from_nbt", |b| {
        b.iter(|| chunk_from_nbt(black_box(&input), -64, 384, black_box(&registry)).unwrap())
    });
}
criterion_group!(benches, chunk_load);
criterion_main!(benches);
