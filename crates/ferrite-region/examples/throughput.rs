//! Run with `cargo run -p ferrite-region --release --example throughput`.
use ferrite_region::{ChunkPos, RegionData, Regionizer, Scheduler, SectionPos};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
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
    for threads in [1, 2, 4] {
        let mut regions = Regionizer::<Data>::new(0, 1);
        for i in 0..16 {
            regions.add_chunk(ChunkPos(i * 4, 0));
        }
        let ticks = Arc::new(AtomicUsize::new(0));
        let counter = ticks.clone();
        let scheduler = Scheduler::new(
            regions,
            (),
            threads,
            move |_| {
                thread::sleep(Duration::from_millis(5));
                counter.fetch_add(1, Ordering::Relaxed);
            },
            |_| {},
        );
        let start = Instant::now();
        thread::sleep(Duration::from_secs(3));
        scheduler.shutdown();
        println!(
            "{threads} threads: {:.1} region ticks/s (16 regions, 5 ms simulated work)",
            ticks.load(Ordering::Relaxed) as f64 / start.elapsed().as_secs_f64()
        );
    }
}
