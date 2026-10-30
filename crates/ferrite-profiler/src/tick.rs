//! Fixed-size, lock-free recording of tick durations.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use crate::stats::{MAX_TPS, Mspt, MsptStats, Tps};

/// Ticks retained per window: 15 minutes at 20 TPS.
const RING_CAPACITY: usize = 18_000;
const WINDOW_5S: usize = 100;
const WINDOW_10S: usize = 200;
const WINDOW_1M: usize = 1_200;
const WINDOW_5M: usize = 6_000;
const WINDOW_15M: usize = RING_CAPACITY;

/// Records tick durations per region and globally.
///
/// Every tick is stored in a fixed-size ring of `u32` microseconds, so
/// recording never allocates and never blocks: writers reserve a slot with one
/// `fetch_add` and publish the duration with one relaxed store. Readers
/// (`tps`, `mspt`) take a snapshot of the newest samples and compute their
/// statistics on the caller's thread; a tick that is being written while a
/// report is taken may be read as the previous value, which is harmless for
/// averaged windows.
///
/// The default monitor has 16 region slots. Each slot (like the global ring)
/// costs about 70 KiB, so `TickMonitor::new()` allocates roughly 1.1 MiB once.
/// Region ids outside `0..regions()` are still recorded globally.
///
/// # Example
///
/// ```
/// use std::time::Duration;
/// use ferrite_profiler::TickMonitor;
///
/// let monitor = TickMonitor::new();
/// monitor.record(0, Duration::from_millis(50));
/// assert_eq!(monitor.mspt().last_10s.samples, 1);
/// ```
pub struct TickMonitor {
    global: TickRing,
    regions: Box<[TickRing]>,
}

impl TickMonitor {
    /// Number of region slots allocated by [`TickMonitor::new`].
    pub const DEFAULT_REGIONS: usize = 16;

    /// Creates a monitor with [`TickMonitor::DEFAULT_REGIONS`] region slots.
    pub fn new() -> Self {
        Self::with_regions(Self::DEFAULT_REGIONS)
    }

    /// Creates a monitor with room for `regions` region ids.
    ///
    /// # Panics
    ///
    /// Panics when `regions` is zero.
    pub fn with_regions(regions: usize) -> Self {
        assert!(regions > 0, "TickMonitor needs at least one region slot");
        Self {
            global: TickRing::new(),
            regions: (0..regions).map(|_| TickRing::new()).collect(),
        }
    }

    /// Number of region slots.
    pub fn regions(&self) -> usize {
        self.regions.len()
    }

    /// Records one regional tick in both the region window and the global window.
    ///
    /// Out-of-range region ids are only counted globally.
    pub fn record(&self, region: u32, duration: Duration) {
        self.global.push(duration);
        if let Some(ring) = self.regions.get(region as usize) {
            ring.push(duration);
        }
    }

    /// Records one tick in the global window only.
    ///
    /// Use this instead of [`TickMonitor::record`] when the server has a
    /// dedicated global tick loop next to the region loops.
    pub fn record_global(&self, duration: Duration) {
        self.global.push(duration);
    }

    /// Records one regional tick without touching the global window.
    ///
    /// Region threads of a regionized server record their own durations with
    /// this and leave the global window to the global region's tick loop, so
    /// global TPS stays a rate rather than a mix of every region's durations.
    /// Out-of-range region ids are ignored.
    pub fn record_region(&self, region: u32, duration: Duration) {
        if let Some(ring) = self.regions.get(region as usize) {
            ring.push(duration);
        }
    }

    /// Clears one region window, for example before a slot is handed to a new
    /// region.
    ///
    /// Call this while no thread records into the slot, so the next owner
    /// starts its own window instead of inheriting samples from the previous
    /// one. Out-of-range region ids are ignored.
    pub fn reset_region(&self, region: u32) {
        if let Some(ring) = self.regions.get(region as usize) {
            ring.clear();
        }
    }

    /// Total number of ticks recorded globally.
    pub fn ticks(&self) -> u64 {
        self.global.len()
    }

    /// Total number of ticks recorded for `region`, or `None` if the id is
    /// outside `0..regions()`.
    pub fn region_ticks(&self, region: u32) -> Option<u64> {
        self.regions.get(region as usize).map(TickRing::len)
    }

    /// Global TPS over the four windows.
    pub fn tps(&self) -> Tps {
        ring_tps(&self.global)
    }

    /// TPS for one region, or `None` if the id is outside `0..regions()`.
    pub fn tps_of(&self, region: u32) -> Option<Tps> {
        self.regions.get(region as usize).map(ring_tps)
    }

    /// Global MSPT over the ten-second and one-minute windows.
    pub fn mspt(&self) -> Mspt {
        ring_mspt(&self.global)
    }

    /// MSPT for one region, or `None` if the id is outside `0..regions()`.
    pub fn mspt_of(&self, region: u32) -> Option<Mspt> {
        self.regions.get(region as usize).map(ring_mspt)
    }
}

impl Default for TickMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// One fixed-size ring of tick durations.
struct TickRing {
    slots: Box<[AtomicU32]>,
    /// Total number of ticks ever recorded; the ring slot is `seq % capacity`.
    head: AtomicU64,
}

impl TickRing {
    fn new() -> Self {
        Self {
            slots: (0..RING_CAPACITY).map(|_| AtomicU32::new(0)).collect(),
            head: AtomicU64::new(0),
        }
    }

    fn push(&self, duration: Duration) {
        let micros = duration.as_micros().min(u128::from(u32::MAX)) as u32;
        let sequence = self.head.fetch_add(1, Ordering::Relaxed);
        let slot = (sequence % RING_CAPACITY as u64) as usize;
        self.slots[slot].store(micros, Ordering::Relaxed);
    }

    fn len(&self) -> u64 {
        self.head.load(Ordering::Acquire)
    }

    /// Forgets every sample. Readers see an empty window even if they load the
    /// old head before the slots are zeroed, because a zero duration is read as
    /// a full-TPS tick.
    fn clear(&self) {
        self.head.store(0, Ordering::Release);
        for slot in self.slots.iter() {
            slot.store(0, Ordering::Relaxed);
        }
    }

    /// Copies the newest `window` durations, oldest first, into `out`.
    fn copy_window(&self, window: usize, out: &mut Vec<u32>) {
        let end = self.len();
        let start = end.saturating_sub(window as u64);
        out.clear();
        out.reserve((end - start) as usize);
        for sequence in start..end {
            let slot = (sequence % RING_CAPACITY as u64) as usize;
            out.push(self.slots[slot].load(Ordering::Relaxed));
        }
    }
}

fn ring_tps(ring: &TickRing) -> Tps {
    let mut scratch = Vec::with_capacity(WINDOW_5S);
    let last_5s = window_tps(ring, WINDOW_5S, &mut scratch);
    let last_1m = window_tps(ring, WINDOW_1M, &mut scratch);
    let last_5m = window_tps(ring, WINDOW_5M, &mut scratch);
    let last_15m = window_tps(ring, WINDOW_15M, &mut scratch);
    Tps {
        last_5s,
        last_1m,
        last_5m,
        last_15m,
    }
}

fn window_tps(ring: &TickRing, window: usize, scratch: &mut Vec<u32>) -> f64 {
    ring.copy_window(window, scratch);
    tps_from_micros(scratch)
}

/// `ticks / elapsed_time`, capped at [`MAX_TPS`].
fn tps_from_micros(samples: &[u32]) -> f64 {
    if samples.is_empty() {
        return MAX_TPS;
    }
    let total_micros: u64 = samples.iter().map(|&sample| u64::from(sample)).sum();
    if total_micros == 0 {
        return MAX_TPS;
    }
    let seconds = total_micros as f64 / 1_000_000.0;
    (samples.len() as f64 / seconds).clamp(0.0, MAX_TPS)
}

fn ring_mspt(ring: &TickRing) -> Mspt {
    Mspt {
        last_10s: window_mspt(ring, WINDOW_10S),
        last_1m: window_mspt(ring, WINDOW_1M),
    }
}

fn window_mspt(ring: &TickRing, window: usize) -> MsptStats {
    let mut samples = Vec::with_capacity(window);
    ring.copy_window(window, &mut samples);
    MsptStats::from_micros(&mut samples)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    fn record_millis(monitor: &TickMonitor, count: usize, millis: u64) {
        for _ in 0..count {
            monitor.record_global(Duration::from_millis(millis));
        }
    }

    #[test]
    fn empty_monitor_reports_full_tps_and_no_mspt() {
        let monitor = TickMonitor::new();
        assert_eq!(monitor.ticks(), 0);
        assert_eq!(monitor.tps(), Tps::uniform(MAX_TPS));
        assert_eq!(monitor.mspt(), Mspt::default());
        assert_eq!(monitor.mspt_of(0), Some(Mspt::default()));
    }

    #[test]
    fn tps_is_capped_at_twenty() {
        let monitor = TickMonitor::new();
        record_millis(&monitor, 200, 10);
        assert_eq!(monitor.tps(), Tps::uniform(MAX_TPS));
    }

    #[test]
    fn tps_follows_tick_duration() {
        let monitor = TickMonitor::new();
        record_millis(&monitor, 120, 100);
        let tps = monitor.tps();
        assert!((tps.last_5s - 10.0).abs() < 1e-9, "{tps:?}");
        assert!((tps.last_1m - 10.0).abs() < 1e-9, "{tps:?}");
        assert!((tps.last_5m - 10.0).abs() < 1e-9, "{tps:?}");
        assert!((tps.last_15m - 10.0).abs() < 1e-9, "{tps:?}");
    }

    #[test]
    fn tps_accounts_for_lag_spikes() {
        let monitor = TickMonitor::new();
        record_millis(&monitor, 99, 50);
        record_millis(&monitor, 1, 1_000);

        let tps = monitor.tps();
        // 99 * 50 ms + 1000 ms over 100 ticks is an average tick of 59.5 ms.
        let expected = 1_000.0 / 59.5;
        assert!(expected < MAX_TPS);
        assert!((tps.last_5s - expected).abs() < 1e-9, "{tps:?}");
        assert!((tps.last_1m - expected).abs() < 1e-9, "{tps:?}");
        assert!((tps.last_15m - expected).abs() < 1e-9, "{tps:?}");
    }

    #[test]
    fn tps_windows_use_their_own_tick_ranges() {
        let monitor = TickMonitor::new();
        // Ten one-second stalls, then a minute's worth of healthy ticks.
        record_millis(&monitor, 10, 1_000);
        record_millis(&monitor, 1_190, 50);

        let tps = monitor.tps();
        assert_eq!(tps.last_5s, MAX_TPS, "the newest 100 ticks are healthy");
        let expected = 1_200.0 / 69.5;
        assert!((tps.last_1m - expected).abs() < 1e-9, "{tps:?}");
        assert!((tps.last_5m - expected).abs() < 1e-9, "{tps:?}");
        assert!((tps.last_15m - expected).abs() < 1e-9, "{tps:?}");
    }

    #[test]
    fn mspt_windows_pick_the_right_ticks() {
        let monitor = TickMonitor::new();
        record_millis(&monitor, 1_000, 10);
        for millis in 1..=200 {
            monitor.record_global(Duration::from_millis(millis));
        }

        let mspt = monitor.mspt();
        let ten = mspt.last_10s;
        assert_eq!(ten.samples, 200);
        assert!((ten.avg - 100.5).abs() < 1e-9, "{ten:?}");
        assert_eq!(ten.min, 1.0);
        assert_eq!(ten.max, 200.0);
        assert_eq!(ten.p50, 100.0);
        assert_eq!(ten.p95, 190.0);
        assert_eq!(ten.p99, 198.0);

        let minute = mspt.last_1m;
        assert_eq!(minute.samples, 1_200);
        assert!(
            (minute.avg - 25.083_333_333_333_332).abs() < 1e-9,
            "{minute:?}"
        );
        assert_eq!(minute.min, 1.0);
        assert_eq!(minute.max, 200.0);
    }

    #[test]
    fn regional_recording_leaves_the_global_window_alone() {
        let monitor = TickMonitor::with_regions(2);
        monitor.record_region(1, Duration::from_millis(100));
        monitor.record_region(9, Duration::from_millis(100));

        assert_eq!(monitor.ticks(), 0);
        assert_eq!(monitor.tps(), Tps::uniform(MAX_TPS));
        assert_eq!(monitor.mspt().last_10s.samples, 0);

        assert_eq!(monitor.region_ticks(1), Some(1));
        assert_eq!(monitor.tps_of(1).unwrap().last_5s, 10.0);
        assert_eq!(monitor.mspt_of(1).unwrap().last_10s.samples, 1);
        assert_eq!(monitor.region_ticks(0), Some(0));
    }

    #[test]
    fn reset_region_clears_only_that_slot() {
        let monitor = TickMonitor::with_regions(2);
        monitor.record_region(0, Duration::from_millis(100));
        monitor.record_region(1, Duration::from_millis(100));

        monitor.reset_region(0);
        monitor.reset_region(7);

        assert_eq!(monitor.region_ticks(0), Some(0));
        assert_eq!(monitor.mspt_of(0).unwrap().last_10s.samples, 0);
        assert_eq!(monitor.tps_of(0).unwrap().last_5s, MAX_TPS);
        assert_eq!(monitor.region_ticks(1), Some(1));
        assert_eq!(monitor.mspt_of(1).unwrap().last_10s.samples, 1);
    }

    #[test]
    fn per_region_windows_are_isolated() {
        let monitor = TickMonitor::with_regions(4);
        for _ in 0..3 {
            monitor.record(2, Duration::from_millis(50));
        }

        assert_eq!(monitor.ticks(), 3);
        assert_eq!(monitor.region_ticks(2), Some(3));
        assert_eq!(monitor.tps_of(2).unwrap().last_5s, MAX_TPS);
        assert_eq!(monitor.mspt_of(2).unwrap().last_10s.samples, 3);
        assert_eq!(monitor.mspt().last_10s.samples, 3);

        assert_eq!(monitor.region_ticks(0), Some(0));
        assert_eq!(monitor.mspt_of(0).unwrap().last_10s.samples, 0);
        assert_eq!(monitor.tps_of(0).unwrap().last_5s, MAX_TPS);

        assert_eq!(monitor.tps_of(9), None);
        assert_eq!(monitor.mspt_of(9), None);
        assert_eq!(monitor.region_ticks(9), None);
    }

    #[test]
    fn concurrent_recording_from_eight_threads() {
        let monitor = Arc::new(TickMonitor::with_regions(8));
        let mut handles = Vec::new();
        for region in 0..8u32 {
            let monitor = Arc::clone(&monitor);
            handles.push(thread::spawn(move || {
                for index in 0..500u64 {
                    let millis = 40 + (index + u64::from(region)) % 21;
                    monitor.record(region, Duration::from_millis(millis));
                }
            }));
        }
        for handle in handles {
            handle.join().expect("recording thread panicked");
        }

        assert_eq!(monitor.ticks(), 4_000);
        for region in 0..8u32 {
            assert_eq!(monitor.region_ticks(region), Some(500));
            let stats = monitor.mspt_of(region).unwrap().last_10s;
            assert_eq!(stats.samples, 200);
            assert!(stats.min >= 40.0 && stats.max <= 60.0, "{stats:?}");
        }

        let global = monitor.mspt().last_10s;
        assert_eq!(global.samples, 200);
        assert!(global.min >= 40.0 && global.max <= 60.0, "{global:?}");
        let tps = monitor.tps().last_5s;
        assert!((0.0..=MAX_TPS).contains(&tps), "{tps}");
    }
}
