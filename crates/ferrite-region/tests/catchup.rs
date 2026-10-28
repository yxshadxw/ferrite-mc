//! Bounded catch-up after overload.
use ferrite_region::{ChunkPos, RegionData, Regionizer, Scheduler, SchedulerConfig, SectionPos};
use std::collections::HashSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Data;
impl RegionData for Data {
    fn merge(&mut self, _: Self) {}
    fn split(&mut self, _: &HashSet<SectionPos>) -> Self {
        Self
    }
}

const PERIOD: Duration = Duration::from_millis(10);
const WINDOW: Duration = Duration::from_millis(100);

/// Overloads one region (60 ms ticks at a 10 ms period) for 600 ms, removes the
/// load and returns (ticks started in the first `WINDOW` after the first light
/// tick, skipped ticks reported in stats).
fn overload_then_relax(max_catchup: u32) -> (usize, u64) {
    let mut topology = Regionizer::<Data>::new(0, 1);
    topology.add_chunk(ChunkPos(0, 0));
    let heavy = Arc::new(AtomicBool::new(true));
    let log = Arc::new(Mutex::new(Vec::<(Instant, bool)>::new()));
    let (tick_heavy, tick_log) = (heavy.clone(), log.clone());
    let config = SchedulerConfig {
        threads: 2,
        period: PERIOD,
        max_catchup,
    };
    let scheduler = Scheduler::with_config(
        topology,
        (),
        config,
        move |_: &mut Data| {
            let is_heavy = tick_heavy.load(Ordering::SeqCst);
            tick_log.lock().unwrap().push((Instant::now(), is_heavy));
            if is_heavy {
                thread::sleep(Duration::from_millis(60));
            }
        },
        |_| {},
    );
    thread::sleep(Duration::from_millis(600));
    heavy.store(false, Ordering::SeqCst);
    thread::sleep(WINDOW + Duration::from_millis(150));
    let id = scheduler.owner(ChunkPos(0, 0)).unwrap();
    let skipped = scheduler.stats(id).unwrap().skipped_ticks;
    scheduler.shutdown();
    let log = log.lock().unwrap();
    let first_light = log.iter().find(|(_, h)| !h).unwrap().0;
    let burst = log
        .iter()
        .filter(|(t, h)| !h && *t >= first_light && *t < first_light + WINDOW)
        .count();
    (burst, skipped)
}

#[test]
fn overloaded_region_does_not_burst_after_load_is_removed() {
    let steady = (WINDOW.as_millis() / PERIOD.as_millis()) as usize;
    let (uncapped, uncapped_skipped) = overload_then_relax(u32::MAX);
    let (capped, capped_skipped) = overload_then_relax(1);
    let (capped4, _) = overload_then_relax(4);
    eprintln!(
        "ticks in first {WINDOW:?} after overload (steady rate {steady}): uncapped {uncapped} \
         (skipped {uncapped_skipped}), max_catchup=1 {capped} (skipped {capped_skipped}), \
         max_catchup=4 {capped4}"
    );
    // Without a cap the ~50-tick backlog is replayed back to back.
    assert!(uncapped > steady * 2, "test did not create a backlog");
    assert_eq!(uncapped_skipped, 0);
    // With a cap: at most the steady rate plus `max_catchup` + 1 extra ticks.
    assert!(capped <= steady + 1 + 1, "burst of {capped} ticks");
    assert!(capped4 <= steady + 4 + 1, "burst of {capped4} ticks");
    assert!(capped_skipped >= 30, "skipped ticks not reported");
}
