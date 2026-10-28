//! Random block churn benchmark: cargo run -p ferrite-world --release --example churn
use ferrite_world::{PaletteKind, PalettedContainer};
use std::{hint::black_box, time::Instant};
fn main() {
    let mut section = PalettedContainer::new(PaletteKind::Blocks, 0, 16).unwrap();
    let mut seed = 12345u32;
    let start = Instant::now();
    for _ in 0..20_000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let index = (seed as usize >> 8) % 4096;
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        section.set(index, (seed >> 16) % 512).unwrap();
    }
    println!(
        "random block churn: {:.1} ns/set",
        start.elapsed().as_secs_f64() * 1e9 / 20_000.0
    );
    black_box(section);
}
