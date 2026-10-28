//! Scheduler scaling benchmark, run with
//! `cargo run -p ferrite-region --release --example throughput`.
//!
//! 64 base regions each burn ~10 ms of CPU per tick, so demand is
//! 64 x 20 TPS x 10 ms = 12.8 CPU-seconds per second: no thread count up to 8
//! can reach the 20 TPS cap, and achieved ticks/s measures scaling directly.
//! A background thread performs ~2000 topology ops/s that repeatedly bridge
//! neighbouring regions (merge) and cut the bridges again (split), so many ops
//! hit ticking regions and take the deferred path. Tick cost is per base
//! section, so merged regions do proportionally more work and total demand
//! stays constant.
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

const REGIONS: i32 = 64;
const WORK_PER_TICK: Duration = Duration::from_millis(10);
const OPS_PER_SEC: f64 = 2000.0;
const RUN: Duration = Duration::from_secs(5);

/// Base sections (x = 3i) owned by this region.
#[derive(Default)]
struct Data(HashSet<SectionPos>);
impl RegionData for Data {
    fn merge(&mut self, other: Self) {
        self.0.extend(other.0);
    }
    fn split(&mut self, sections: &HashSet<SectionPos>) -> Self {
        let moved = self.0.intersection(sections).copied().collect();
        self.0.retain(|s| !sections.contains(s));
        Self(moved)
    }
}

fn spin(duration: Duration) {
    let start = Instant::now();
    let mut n = 0u64;
    while start.elapsed() < duration {
        n = black_box(n.wrapping_add(1));
    }
    black_box(n);
}

fn main() {
    let cores = thread::available_parallelism().map_or(1, |n| n.get());
    println!(
        "available_parallelism = {cores}; {REGIONS} regions x {WORK_PER_TICK:?} CPU/tick, \
         ~{OPS_PER_SEC} topology ops/s, {RUN:?} per run"
    );
    println!(
        "| threads | ticks/s | ideal | per-region TPS mean | min | max | ops/s | deferred ops | max pending age |"
    );
    println!("|---|---|---|---|---|---|---|---|---|");
    for threads in [1, 2, 4, 8] {
        run(threads, cores);
    }
}

fn run(threads: usize, cores: usize) {
    let mut topology = Regionizer::<Data>::new(0, 1);
    for i in 0..REGIONS {
        topology.add_chunk(ChunkPos(i * 3, 0));
        let id = topology.owner(ChunkPos(i * 3, 0)).unwrap();
        topology
            .data_mut(id)
            .unwrap()
            .0
            .insert(SectionPos(i * 3, 0));
    }
    let counts: Arc<Vec<AtomicUsize>> =
        Arc::new((0..REGIONS).map(|_| AtomicUsize::new(0)).collect());
    let tick_counts = counts.clone();
    let scheduler = Arc::new(Scheduler::new(
        topology,
        (),
        threads,
        move |data: &mut Data| {
            for section in &data.0 {
                spin(WORK_PER_TICK);
                tick_counts[(section.0 / 3) as usize].fetch_add(1, Ordering::Relaxed);
            }
        },
        |_| {},
    ));

    let running = Arc::new(AtomicBool::new(true));
    let ops = Arc::new(AtomicUsize::new(0));
    let deferred = Arc::new(AtomicUsize::new(0));
    let producer = {
        let (running, ops, deferred, scheduler) = (
            running.clone(),
            ops.clone(),
            deferred.clone(),
            scheduler.clone(),
        );
        thread::spawn(move || {
            // Bridge chunks 3i+1 / 3i+2: holding both merges regions i and i+1.
            let mut held = vec![false; (REGIONS as usize - 1) * 2];
            let mut seed = 0x9e37_79b9_7f4a_7c15u64;
            let start = Instant::now();
            let mut done = 0usize;
            while running.load(Ordering::Relaxed) {
                let due = (start.elapsed().as_secs_f64() * OPS_PER_SEC) as usize;
                while done < due {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    let slot = (seed % held.len() as u64) as usize;
                    let x = (slot / 2) as i32 * 3 + 1 + (slot % 2) as i32;
                    let chunk = ChunkPos(x, 0);
                    let before = scheduler.pending_mutations();
                    if held[slot] {
                        scheduler.remove_chunk(chunk);
                    } else {
                        scheduler.add_chunk(chunk);
                    }
                    if scheduler.pending_mutations() > before {
                        deferred.fetch_add(1, Ordering::Relaxed);
                    }
                    held[slot] = !held[slot];
                    done += 1;
                }
                ops.store(done, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(1));
            }
        })
    };

    // Warm up, then measure a window.
    thread::sleep(Duration::from_millis(500));
    let base: Vec<usize> = counts.iter().map(|c| c.load(Ordering::Relaxed)).collect();
    let ops_base = ops.load(Ordering::Relaxed);
    let deferred_base = deferred.load(Ordering::Relaxed);
    let start = Instant::now();
    let mut max_age = Duration::ZERO;
    while start.elapsed() < RUN {
        if let Some(age) = scheduler.oldest_pending_mutation() {
            max_age = max_age.max(age);
        }
        thread::sleep(Duration::from_millis(5));
    }
    let elapsed = start.elapsed().as_secs_f64();
    let per_region: Vec<f64> = counts
        .iter()
        .zip(&base)
        .map(|(c, b)| (c.load(Ordering::Relaxed) - b) as f64 / elapsed)
        .collect();
    let ops_rate = (ops.load(Ordering::Relaxed) - ops_base) as f64 / elapsed;
    let deferred_count = deferred.load(Ordering::Relaxed) - deferred_base;
    running.store(false, Ordering::Relaxed);
    producer.join().unwrap();
    Arc::try_unwrap(scheduler).ok().unwrap().shutdown();

    let total: f64 = per_region.iter().sum();
    let min = per_region.iter().copied().fold(f64::INFINITY, f64::min);
    let max = per_region.iter().copied().fold(0.0, f64::max);
    let ideal = threads.min(cores) as f64 / WORK_PER_TICK.as_secs_f64();
    println!(
        "| {threads} | {total:.1} | {ideal:.0} | {:.2} | {min:.2} | {max:.2} | {ops_rate:.0} | \
         {deferred_count} | {max_age:.1?} |",
        total / REGIONS as f64
    );
}
