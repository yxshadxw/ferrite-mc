//! Server-side worldgen throughput benchmark.
//!
//! Usage: cargo run --release -p ferrite-server --example genbench [seed] [count] [startX] [startZ]
//!
//! Exercises the same entry point the chunk source uses (`VanillaTerrain::generate`) and
//! times the generated column, the wire encoding of `level_chunk_with_light`, and the
//! whole pipeline. `FERRITE_WORLDGEN_THREADS` selects both the worker pool size and the
//! number of concurrent requesters, so the benchmark measures the pool the server runs.

use std::sync::Arc;
use std::time::Instant;

fn worldgen_threads() -> usize {
    std::env::var("FERRITE_WORLDGEN_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|threads| *threads > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(1, |threads| threads.get().min(8))
        })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: i64 = args.get(1).map_or(0, |s| s.parse().unwrap());
    let count: usize = args.get(2).map_or(50, |s| s.parse().unwrap());
    let start_x: i32 = args.get(3).map_or(0, |s| s.parse().unwrap());
    let start_z: i32 = args.get(4).map_or(0, |s| s.parse().unwrap());
    let requesters = worldgen_threads();

    let terrain = Arc::new(ferrite_server::game::VanillaTerrain::spawn(seed).unwrap());
    let started = Instant::now();
    let (generate_total, encode_total) = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(requesters);
        for requester in 0..requesters {
            let terrain = Arc::clone(&terrain);
            handles.push(scope.spawn(move || {
                let mut generate = 0.0_f64;
                let mut encode = 0.0_f64;
                for index in (requester..count).step_by(requesters) {
                    let cx = start_x + (index % 10) as i32;
                    let cz = start_z + (index / 10) as i32;
                    let mark = Instant::now();
                    let generated = terrain.generate(cx, cz).unwrap();
                    generate += mark.elapsed().as_secs_f64() * 1000.0;
                    let mark = Instant::now();
                    let packet =
                        ferrite_world::encode_chunk_packet(cx, cz, &generated.chunk, &generated.light)
                            .unwrap();
                    encode += mark.elapsed().as_secs_f64() * 1000.0;
                    std::hint::black_box(packet.len());
                }
                (generate, encode)
            }));
        }
        handles.into_iter().fold((0.0, 0.0), |totals, handle| {
            let (generate, encode) = handle.join().unwrap();
            (totals.0 + generate, totals.1 + encode)
        })
    });
    let wall = started.elapsed().as_secs_f64();
    let n = count.max(1) as f64;
    println!(
        "server worldgen benchmark: seed={seed} chunks={count} from ({start_x},{start_z}) threads={requesters}"
    );
    println!("generate+light: {:8.2} ms/chunk (cpu)", generate_total / n);
    println!("wire encode:    {:8.2} ms/chunk (cpu)", encode_total / n);
    println!("pipeline wall:  {:8.2} ms/chunk", wall * 1000.0 / n);
    println!("chunks/second:  {:8.2}", count as f64 / wall);
}
