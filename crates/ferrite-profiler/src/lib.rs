//! Tick, scope and sampling profiler for Ferrite (spark/Paper-timings style).
//!
//! The crate is split into three independent pieces:
//!
//! * [`TickMonitor`] keeps per-region and global tick durations in fixed-size
//!   lock-free ring buffers and derives TPS and MSPT windows from them.
//! * [`Timings`] records hierarchical scopes with an RAII guard, aggregated per
//!   region and reported as a tree sorted by total time.
//! * [`Sampler`] walks registered threads every few milliseconds, reads the
//!   scopes they are currently inside and aggregates a flame-graph-like tree
//!   that exports as text or JSON.
//!
//! Nothing here allocates on the tick path, and every shared counter is an
//! atomic so the region scheduler can record from any thread. [`paper`] turns
//! the numbers into the colored chat lines used by `/tps` and `/mspt`.
//!
//! ```
//! use std::time::Duration;
//! use ferrite_profiler::{TickMonitor, Timings, paper};
//!
//! let timings = Timings::with_regions(4);
//! timings.set_thread_region(1);
//! {
//!     let _tick = timings.scope("tick");
//!     {
//!         let _entity = timings.scope("entity-tick");
//!     }
//! }
//! let report = timings.report(1, 50.0);
//! assert_eq!(report.rows[1].path, "tick/entity-tick");
//!
//! let monitor = TickMonitor::with_regions(4);
//! monitor.record(1, Duration::from_millis(50));
//! assert_eq!(
//!     paper::tps_line(&monitor.tps()),
//!     "TPS from last 1m, 5m, 15m: 20.0, 20.0, 20.0"
//! );
//! ```

#![warn(missing_docs)]

pub mod paper;
mod sampler;
mod stats;
mod tick;
mod timings;

pub use paper::{
    mspt_line, mspt_line_colored, mspt_lines, mspt_lines_colored, region_tps_line,
    region_tps_line_colored, tps_line, tps_line_colored,
};
pub use sampler::{SampleNode, SampleTree, Sampler};
pub use stats::{MAX_TPS, Mspt, MsptStats, Tps};
pub use tick::TickMonitor;
pub use timings::{
    DEFAULT_REGIONS, GLOBAL_REGION, MAX_DEPTH, ReportRow, ScopeGuard, Timings, TimingsReport,
};
