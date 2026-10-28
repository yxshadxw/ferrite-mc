//! Tick-rate and tick-duration statistics.

/// Highest tick rate reported by [`Tps`].
///
/// A Minecraft server ticks once every 50 ms, so `/tps` caps the value at 20
/// even when the recorded tick durations would imply a faster rate.
pub const MAX_TPS: f64 = 20.0;

/// Tick rate over the profiler's four windows, in ticks per second.
///
/// Each window averages the last *N* recorded tick durations, where `N` is
/// 100, 1200, 6000 and 18 000 for 5 s, 1 m, 5 m and 15 m at 20 TPS. A window
/// without samples reports [`MAX_TPS`], matching Paper's `/tps` on a freshly
/// started server.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tps {
    /// Rate over the last 5 seconds.
    pub last_5s: f64,
    /// Rate over the last minute.
    pub last_1m: f64,
    /// Rate over the last 5 minutes.
    pub last_5m: f64,
    /// Rate over the last 15 minutes.
    pub last_15m: f64,
}

impl Tps {
    /// A report where every window has the same rate.
    pub const fn uniform(tps: f64) -> Self {
        Self {
            last_5s: tps,
            last_1m: tps,
            last_5m: tps,
            last_15m: tps,
        }
    }
}

/// Tick-duration statistics in milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MsptStats {
    /// Number of ticks in the window.
    pub samples: usize,
    /// Mean tick duration.
    pub avg: f64,
    /// Shortest tick.
    pub min: f64,
    /// Longest tick.
    pub max: f64,
    /// Median tick duration.
    pub p50: f64,
    /// 95th percentile.
    pub p95: f64,
    /// 99th percentile.
    pub p99: f64,
}

impl MsptStats {
    /// Builds the statistics from raw tick durations in microseconds.
    ///
    /// The slice is sorted in place. Percentiles use the nearest-rank method:
    /// for `1..=200` the median is 100, `p95` is 190 and `p99` is 198.
    pub fn from_micros(samples: &mut [u32]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        samples.sort_unstable();
        let len = samples.len();
        let sum: u64 = samples.iter().map(|&sample| u64::from(sample)).sum();
        Self {
            samples: len,
            avg: sum as f64 / len as f64 / 1000.0,
            min: micros_to_ms(samples[0]),
            max: micros_to_ms(samples[len - 1]),
            p50: percentile_ms(samples, 0.50),
            p95: percentile_ms(samples, 0.95),
            p99: percentile_ms(samples, 0.99),
        }
    }
}

/// [`MsptStats`] for the ten-second and one-minute windows.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Mspt {
    /// Last 10 seconds (up to 200 ticks).
    pub last_10s: MsptStats,
    /// Last minute (up to 1200 ticks).
    pub last_1m: MsptStats,
}

/// Nearest-rank percentile of an ascending slice, converted to milliseconds.
fn percentile_ms(sorted: &[u32], percentile: f64) -> f64 {
    let rank = (percentile * sorted.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    micros_to_ms(sorted[index])
}

fn micros_to_ms(micros: u32) -> f64 {
    f64::from(micros) / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_samples_are_all_zero() {
        let mut samples: [u32; 0] = [];
        assert_eq!(MsptStats::from_micros(&mut samples), MsptStats::default());
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let mut samples = [4_000u32, 1_000, 3_000, 2_000];
        let stats = MsptStats::from_micros(&mut samples);
        assert_eq!(stats.samples, 4);
        assert_eq!(stats.avg, 2.5);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 4.0);
        assert_eq!(stats.p50, 2.0);
        assert_eq!(stats.p95, 4.0);
        assert_eq!(stats.p99, 4.0);
    }

    #[test]
    fn percentile_of_one_hundred_values() {
        let mut samples: Vec<u32> = (1..=100).map(|value| value * 1_000).collect();
        let stats = MsptStats::from_micros(&mut samples);
        assert_eq!(stats.p50, 50.0);
        assert_eq!(stats.p95, 95.0);
        assert_eq!(stats.p99, 99.0);
        assert_eq!(stats.avg, 50.5);
    }

    #[test]
    fn percentile_of_two_hundred_values() {
        let mut samples: Vec<u32> = (1..=200).map(|value| value * 1_000).collect();
        let stats = MsptStats::from_micros(&mut samples);
        assert_eq!(stats.p50, 100.0);
        assert_eq!(stats.p95, 190.0);
        assert_eq!(stats.p99, 198.0);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 200.0);
        assert_eq!(stats.avg, 100.5);
    }

    #[test]
    fn single_sample_is_every_percentile() {
        let mut samples = [42_000u32];
        let stats = MsptStats::from_micros(&mut samples);
        assert_eq!(stats.samples, 1);
        assert_eq!(stats.avg, 42.0);
        assert_eq!(stats.min, 42.0);
        assert_eq!(stats.max, 42.0);
        assert_eq!(stats.p50, 42.0);
        assert_eq!(stats.p95, 42.0);
        assert_eq!(stats.p99, 42.0);
    }
}
