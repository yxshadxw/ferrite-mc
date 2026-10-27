//! `cargo run -p ferrite-light --release --example throughput`
use ferrite_light::{LightBlockView, LightEngine};
use std::{hint::black_box, time::Instant};
struct Terrain {
    torch: bool,
    stone: u16,
    torch_id: u16,
}
impl LightBlockView for Terrain {
    fn state(&self, x: i32, y: i32, z: i32) -> u16 {
        if self.torch && (x, y, z) == (8, 65, 8) {
            return self.torch_id;
        }
        let ground = 60 + (x.wrapping_mul(13) ^ z.wrapping_mul(7)).rem_euclid(7);
        if y < ground && !(y > 0 && y < 28 && (x * x + z * z + y * y).rem_euclid(47) < 6) {
            self.stone
        } else {
            0
        }
    }
    fn is_chunk_loaded(&self, x: i32, z: i32) -> bool {
        (x, z) == (0, 0)
    }
}
fn main() {
    let mut terrain = Terrain {
        torch: false,
        stone: ferrite_data::blocks::default_state("stone").unwrap(),
        torch_id: ferrite_data::blocks::default_state("torch").unwrap(),
    };
    let mut light = LightEngine::new(0, 0, -4, 19, true);
    let start = Instant::now();
    light.relight(&terrain);
    println!(
        "24-section full lighting: {:.3} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let start = Instant::now();
    for _ in 0..100 {
        terrain.torch = true;
        light.block_changed(&terrain, 8, 65, 8);
        terrain.torch = false;
        light.block_changed(&terrain, 8, 65, 8);
    }
    println!(
        "torch place/remove: {:.3} ms per change",
        start.elapsed().as_secs_f64() * 1000.0 / 200.0
    );
    black_box(light);
}
