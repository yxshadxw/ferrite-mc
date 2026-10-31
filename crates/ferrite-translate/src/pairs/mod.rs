//! One module per adjacent version pair, newest first. Each module owns its generated
//! tables (see `build.rs`), its `fallbacks.txt` and the body rewrites for packets whose
//! layout changed between its two versions.

pub mod v777_v776;

use crate::VersionPair;

/// Every pair, newest first: a translator chains them from the core version down.
pub static PAIRS: &[&VersionPair] = &[&v777_v776::PAIR];
