//! Server-side worldgen throughput benchmark.
//!
//! Usage: cargo run --release -p ferrite-server --example genbench [seed] [count] [startX] [startZ]
//!
//! Exercises the same entry point the chunk source uses (`VanillaTerrain::generate`) and
//! times the generated column, the wire encoding of `level_chunk_with_light`, and the
//! whole pipeline. Runs single-threaded unless FERRITE_WORLDGEN_THREADS is set, in which
//! case the worker pool is used.

use std::sync::Arc;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: i64 = args.get(1).map_or(0, |s| s.parse().unwrap());
    let count: usize = args.get(2).map_or(50, |s| s.parse().unwrap());
    let start_x: i32 = args.get(3).map_or(0, |s| s.parse().unwrap());
    let start_z: i32 = args.get(4).map_or(0, |s| s.parse().unwrap());

    let terrain = Arc::new(ferrite_server::game::VanillaTerrain::spawn(seed).unwrap());
    let mut generate = 0.0_f64;
    let mut encode = 0.0_f64;
    let started = Instant::now();
    for index in 0..count {
        let cx = start_x + (index % 10) as i32;
        let cz = start_z + (index / 10) as i32;
        let mark = Instant::now();
        let generated = terrain.generate(cx, cz).unwrap();
        generate += mark.elapsed().as_secs_f64() * 1000.0;
        let mark = Instant::now();
        let packet = ferrite_world::encode_chunk_packet(
            cx,
            cz,
            &generated.chunk,
            &generated.light,
        )
        .unwrap();
        encode += mark.elapsed().as_secs_f64() * 1000.0;
        std::hint::black_box(packet.len());
    }
    let wall = started.elapsed().as_secs_f64();
    let n = count.max(1) as f64;
    println!("server worldgen benchmark: seed={seed} chunks={count} from ({start_x},{start_z})");
    println!("generate+light: {:8.2} ms/chunk", generate / n);
    println!("wire encode:    {:8.2} ms/chunk", encode / n);
    println!("pipeline:       {:8.2} ms/chunk", (generate + encode) / n);
    println!("chunks/second:  {:8.2}", count as f64 / wall);
}
