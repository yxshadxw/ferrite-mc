//! Run with `cargo run -p ferrite-region --release --example throughput`.
use ferrite_region::{ChunkPos, RegionData, Regionizer, Scheduler, SectionPos};
use std::{
    collections::HashSet,
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Default)]
struct Data;
impl RegionData for Data {
    fn merge(&mut self, _: Self) {}
    fn split(&mut self, _: &HashSet<SectionPos>) -> Self {
        Self
    }
}
fn main() {
    for threads in [1, 2, 4, 8] {
        let mut regions = Regionizer::<Data>::new(0, 1);
        for i in 0..64 {
            regions.add_chunk(ChunkPos(i * 10, 0));
        }
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = ticks.clone();
        let scheduler = Arc::new(Scheduler::new(
            regions,
            (),
            threads,
            move |_| {
                let start = Instant::now();
                let mut n = 0u64;
                while start.elapsed() < Duration::from_millis(1) {
                    n = black_box(n.wrapping_add(1));
                }
                black_box(n);
                counter.fetch_add(1, Ordering::Relaxed);
            },
            |_| {},
        ));
        let running = Arc::new(AtomicBool::new(true));
        let running_worker = running.clone();
        let scheduler_worker = scheduler.clone();
        let mutations = Arc::new(AtomicUsize::new(0));
        let mutations_worker = mutations.clone();
        let producer = thread::spawn(move || {
            let mut next = Instant::now();
            let mut add = true;
            while running_worker.load(Ordering::Relaxed) {
                if Instant::now() < next {
                    thread::yield_now();
                    continue;
                }
                if add {
                    scheduler_worker.add_chunk(ChunkPos(1, 0));
                } else {
                    scheduler_worker.remove_chunk(ChunkPos(1, 0));
                }
                add = !add;
                mutations_worker.fetch_add(1, Ordering::Relaxed);
                next += Duration::from_micros(500);
            }
        });
        let start = Instant::now();
        thread::sleep(Duration::from_secs(4));
        running.store(false, Ordering::Relaxed);
        producer.join().unwrap();
        Arc::try_unwrap(scheduler).ok().unwrap().shutdown();
        let elapsed = start.elapsed().as_secs_f64();
        println!(
            "{threads} workers: {:.1} ticks/s, {:.1} mutations/s (64 regions, 1 ms CPU)",
            ticks.load(Ordering::Relaxed) as f64 / elapsed,
            mutations.load(Ordering::Relaxed) as f64 / elapsed
        );
    }
}
