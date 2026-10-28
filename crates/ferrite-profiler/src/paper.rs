//! Paper-like chat formatting for `/tps` and `/mspt`.
//!
//! The plain forms return the text without Minecraft formatting codes; the
//! `_colored` forms prefix labels with `§6` and color each value green (`§a`),
//! yellow (`§e`) or red (`§c`) depending on how healthy it is. Strings end in
//! `§r`, so they can be dropped into a chat component without leaking color.

use crate::stats::{Mspt, MsptStats, Tps};

/// `§6`: dark gold, used for the label of a line.
pub const LABEL: &str = "§6";
/// `§a`: green, a healthy value.
pub const GOOD: &str = "§a";
/// `§e`: yellow, a degraded value.
pub const WARN: &str = "§e";
/// `§c`: red, a failing value.
pub const BAD: &str = "§c";
/// `§7`: gray, used for metric labels inside a line.
pub const MUTED: &str = "§7";
/// `§r`: resets formatting.
pub const RESET: &str = "§r";

/// Paper's `/tps` line without formatting codes.
///
/// ```
/// use ferrite_profiler::paper::tps_line;
/// use ferrite_profiler::Tps;
///
/// assert_eq!(
///     tps_line(&Tps::uniform(20.0)),
///     "TPS from last 1m, 5m, 15m: 20.0, 20.0, 20.0"
/// );
/// ```
pub fn tps_line(tps: &Tps) -> String {
    format!(
        "TPS from last 1m, 5m, 15m: {:.1}, {:.1}, {:.1}",
        tps.last_1m, tps.last_5m, tps.last_15m
    )
}

/// Paper's `/tps` line with `§` color codes.
///
/// Values are green at 18 TPS or more, yellow at 15 TPS and red below.
pub fn tps_line_colored(tps: &Tps) -> String {
    let mut line = String::with_capacity(96);
    line.push_str(LABEL);
    line.push_str("TPS from last 1m, 5m, 15m: ");
    for (index, value) in [tps.last_1m, tps.last_5m, tps.last_15m]
        .into_iter()
        .enumerate()
    {
        if index > 0 {
            line.push_str(MUTED);
            line.push_str(", ");
        }
        line.push_str(tps_color(value));
        line.push_str(&format!("{value:.1}"));
    }
    line.push_str(RESET);
    line
}

/// One `/mspt` line without formatting codes.
///
/// `window` is the human-readable window label, for example `10s` or `1m`.
///
/// ```
/// use ferrite_profiler::paper::mspt_line;
/// use ferrite_profiler::MsptStats;
///
/// let stats = MsptStats {
///     samples: 200,
///     avg: 10.0,
///     min: 2.5,
///     max: 60.0,
///     p50: 9.0,
///     p95: 20.0,
///     p99: 50.0,
/// };
/// assert_eq!(
///     mspt_line(&stats, "10s"),
///     "MSPT from last 10s: avg 10.00, min 2.50, max 60.00, p50 9.00, p95 20.00, p99 50.00"
/// );
/// ```
pub fn mspt_line(stats: &MsptStats, window: &str) -> String {
    format!(
        "MSPT from last {window}: avg {:.2}, min {:.2}, max {:.2}, p50 {:.2}, p95 {:.2}, p99 {:.2}",
        stats.avg, stats.min, stats.max, stats.p50, stats.p95, stats.p99
    )
}

/// One `/mspt` line with `§` color codes.
///
/// Values are green at 40 ms or less, yellow at 50 ms and red above.
pub fn mspt_line_colored(stats: &MsptStats, window: &str) -> String {
    let mut line = String::with_capacity(180);
    line.push_str(LABEL);
    line.push_str("MSPT from last ");
    line.push_str(window);
    line.push_str(": ");
    let metrics = [
        ("avg", stats.avg),
        ("min", stats.min),
        ("max", stats.max),
        ("p50", stats.p50),
        ("p95", stats.p95),
        ("p99", stats.p99),
    ];
    for (index, (label, value)) in metrics.into_iter().enumerate() {
        if index > 0 {
            line.push_str(MUTED);
            line.push_str(", ");
        }
        line.push_str(MUTED);
        line.push_str(label);
        line.push(' ');
        line.push_str(mspt_color(value));
        line.push_str(&format!("{value:.2}"));
    }
    line.push_str(RESET);
    line
}

/// Both `/mspt` windows, one per line, without formatting codes.
pub fn mspt_lines(mspt: &Mspt) -> String {
    let mut text = mspt_line(&mspt.last_10s, "10s");
    text.push('\n');
    text.push_str(&mspt_line(&mspt.last_1m, "1m"));
    text
}

/// Both `/mspt` windows, one per line, with `§` color codes.
pub fn mspt_lines_colored(mspt: &Mspt) -> String {
    let mut text = mspt_line_colored(&mspt.last_10s, "10s");
    text.push('\n');
    text.push_str(&mspt_line_colored(&mspt.last_1m, "1m"));
    text
}

fn tps_color(tps: f64) -> &'static str {
    if tps >= 18.0 {
        GOOD
    } else if tps >= 15.0 {
        WARN
    } else {
        BAD
    }
}

fn mspt_color(mspt: f64) -> &'static str {
    if mspt <= 40.0 {
        GOOD
    } else if mspt <= 50.0 {
        WARN
    } else {
        BAD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_tps_line_matches_paper() {
        assert_eq!(
            tps_line(&Tps::uniform(20.0)),
            "TPS from last 1m, 5m, 15m: 20.0, 20.0, 20.0"
        );
    }

    #[test]
    fn plain_tps_line_uses_one_decimal() {
        let tps = Tps {
            last_5s: 20.0,
            last_1m: 19.5,
            last_5m: 10.25,
            last_15m: 4.96,
        };
        assert_eq!(tps_line(&tps), "TPS from last 1m, 5m, 15m: 19.5, 10.2, 5.0");
    }

    #[test]
    fn colored_tps_line_colors_each_value() {
        let tps = Tps {
            last_5s: 20.0,
            last_1m: 20.0,
            last_5m: 16.0,
            last_15m: 10.0,
        };
        let line = tps_line_colored(&tps);
        assert!(line.starts_with("§6TPS from last 1m, 5m, 15m: "), "{line}");
        assert!(line.contains("§a20.0"), "{line}");
        assert!(line.contains("§e16.0"), "{line}");
        assert!(line.contains("§c10.0"), "{line}");
        assert!(line.ends_with("§r"), "{line}");
    }

    #[test]
    fn plain_mspt_line_lists_every_metric() {
        let stats = MsptStats {
            samples: 200,
            avg: 10.0,
            min: 2.5,
            max: 60.0,
            p50: 9.0,
            p95: 20.0,
            p99: 50.0,
        };
        assert_eq!(
            mspt_line(&stats, "10s"),
            "MSPT from last 10s: avg 10.00, min 2.50, max 60.00, p50 9.00, p95 20.00, p99 50.00"
        );
    }

    #[test]
    fn colored_mspt_line_colors_by_tick_budget() {
        let stats = MsptStats {
            samples: 200,
            avg: 10.0,
            min: 2.5,
            max: 60.0,
            p50: 9.0,
            p95: 20.0,
            p99: 50.0,
        };
        let line = mspt_line_colored(&stats, "10s");
        assert!(line.starts_with("§6MSPT from last 10s: "), "{line}");
        assert!(line.contains("§a10.00"), "{line}");
        assert!(line.contains("§c60.00"), "{line}");
        assert!(line.contains("§e50.00"), "{line}");
        assert!(line.ends_with("§r"), "{line}");
    }

    #[test]
    fn mspt_lines_cover_both_windows() {
        let mspt = Mspt {
            last_10s: MsptStats {
                samples: 10,
                avg: 10.0,
                ..MsptStats::default()
            },
            last_1m: MsptStats {
                samples: 20,
                avg: 20.0,
                ..MsptStats::default()
            },
        };
        let text = mspt_lines(&mspt);
        assert!(text.contains("last 10s: avg 10.00"), "{text}");
        assert!(text.contains("last 1m: avg 20.00"), "{text}");
        assert_eq!(text.lines().count(), 2);
    }
}
