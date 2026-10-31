//! Lookup types over the tables `build.rs` generates for each version pair.
//!
//! "Down" maps an ID of the newer version to the older one (clientbound), "up" the other
//! way (serverbound).

use ferrite_protocol::{Bound, State};

/// Remap of one static registry (e.g. `minecraft:item`) between two versions.
#[derive(Debug)]
pub struct RegistryTable {
    pub name: &'static str,
    /// New ID -> old ID matched by name or fallback rule, `-1` if neither matched.
    pub down: &'static [i32],
    /// Old ID -> new ID matched by name, `-1` if the entry was removed.
    pub up: &'static [i32],
    /// The older version's default entry (`-1` if the registry has none).
    pub down_default: i32,
    /// The newer version's default entry (`-1` if the registry has none).
    pub up_default: i32,
}

impl RegistryTable {
    /// Exact or fallback match only; `None` if the older version has no counterpart.
    pub fn down(&self, id: i32) -> Option<i32> {
        lookup(self.down, id)
    }

    /// Like [`Self::down`], falling back to the registry default (e.g. air, pig).
    pub fn down_or_default(&self, id: i32) -> Option<i32> {
        self.down(id)
            .or((self.down_default >= 0).then_some(self.down_default))
    }

    pub fn up(&self, id: i32) -> Option<i32> {
        lookup(self.up, id)
    }

    pub fn up_or_default(&self, id: i32) -> Option<i32> {
        self.up(id)
            .or((self.up_default >= 0).then_some(self.up_default))
    }
}

fn lookup(table: &[i32], id: i32) -> Option<i32> {
    usize::try_from(id)
        .ok()
        .and_then(|i| table.get(i))
        .copied()
        .filter(|&v| v >= 0)
}

/// A rule from a pair's `fallbacks.txt`: entries of `registry` matching `pattern` (with one
/// optional `*` wildcard) map to `target` (with the capture substituted for `*`).
#[derive(Debug)]
pub struct Fallback {
    pub registry: &'static str,
    pub pattern: &'static str,
    pub target: &'static str,
}

/// First rule matching `name` in `registry`, same algorithm as `build.rs`.
pub fn apply_fallback(rules: &[Fallback], registry: &str, name: &str) -> Option<String> {
    rules
        .iter()
        .filter(|rule| rule.registry == registry)
        .find_map(|rule| match rule.pattern.split_once('*') {
            Some((prefix, suffix)) => (name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix))
            .then(|| {
                rule.target
                    .replace('*', &name[prefix.len()..name.len() - suffix.len()])
            }),
            None => (rule.pattern == name).then(|| rule.target.to_owned()),
        })
}

/// Everything generated for one version pair.
#[derive(Debug)]
pub struct PairTables {
    /// Packet names by ID, indexed by [`slot`].
    pub new_packets: &'static [&'static [&'static str]; 10],
    pub old_packets: &'static [&'static [&'static str]; 10],
    /// New packet ID -> old packet ID of the same name (`-1`: no such packet).
    pub packets_down: &'static [&'static [i32]; 10],
    pub packets_up: &'static [&'static [i32]; 10],
    /// New block state ID -> closest old block state ID.
    pub block_states_down: &'static [u32],
    /// Old block state ID -> new block state ID.
    pub block_states_up: &'static [u32],
    /// Static registries present in both versions, sorted by name.
    pub registries: &'static [RegistryTable],
    /// Names of the older version's static registries, sorted.
    pub old_static_registries: &'static [&'static str],
    /// Synchronized data-driven registries of the older version and their vanilla entries.
    pub old_data_registries: &'static [(&'static str, &'static [&'static str])],
    pub fallbacks: &'static [Fallback],
}

/// Index of a (state, direction) packet namespace in the generated tables.
pub fn slot(state: State, bound: Bound) -> usize {
    let state = match state {
        State::Handshaking => 0,
        State::Status => 1,
        State::Login => 2,
        State::Configuration => 3,
        State::Play => 4,
    };
    state * 2 + usize::from(bound == Bound::Clientbound)
}

impl PairTables {
    pub fn new_packet_name(&self, state: State, bound: Bound, id: i32) -> Option<&'static str> {
        name(self.new_packets[slot(state, bound)], id)
    }

    pub fn old_packet_name(&self, state: State, bound: Bound, id: i32) -> Option<&'static str> {
        name(self.old_packets[slot(state, bound)], id)
    }

    pub fn new_packet_id(&self, state: State, bound: Bound, name: &str) -> Option<i32> {
        position(self.new_packets[slot(state, bound)], name)
    }

    pub fn old_packet_id(&self, state: State, bound: Bound, name: &str) -> Option<i32> {
        position(self.old_packets[slot(state, bound)], name)
    }

    /// Old ID of the packet with the same name as new packet `id`.
    pub fn packet_down(&self, state: State, bound: Bound, id: i32) -> Option<i32> {
        lookup(self.packets_down[slot(state, bound)], id)
    }

    /// New ID of the packet with the same name as old packet `id`.
    pub fn packet_up(&self, state: State, bound: Bound, id: i32) -> Option<i32> {
        lookup(self.packets_up[slot(state, bound)], id)
    }

    /// Closest old block state; unknown IDs become air.
    pub fn block_state_down(&self, id: u32) -> u32 {
        self.block_states_down
            .get(id as usize)
            .copied()
            .unwrap_or(0)
    }

    pub fn block_state_up(&self, id: u32) -> u32 {
        self.block_states_up.get(id as usize).copied().unwrap_or(0)
    }

    /// Number of block states in the older version (its global palette size).
    pub fn old_block_state_count(&self) -> usize {
        self.block_states_up.len()
    }

    pub fn new_block_state_count(&self) -> usize {
        self.block_states_down.len()
    }

    pub fn registry(&self, name: &str) -> Option<&'static RegistryTable> {
        self.registries
            .binary_search_by(|r| r.name.cmp(name))
            .ok()
            .map(|i| &self.registries[i])
    }

    /// Vanilla entries of a synchronized registry in the older version, `None` if the older
    /// version does not synchronize that registry.
    pub fn old_data_entries(&self, registry: &str) -> Option<&'static [&'static str]> {
        self.old_data_registries
            .binary_search_by(|(name, _)| (*name).cmp(registry))
            .ok()
            .map(|i| self.old_data_registries[i].1)
    }

    pub fn old_has_static_registry(&self, registry: &str) -> bool {
        self.old_static_registries.binary_search(&registry).is_ok()
    }
}

fn name(table: &'static [&'static str], id: i32) -> Option<&'static str> {
    usize::try_from(id)
        .ok()
        .and_then(|i| table.get(i))
        .copied()
        .filter(|n| !n.is_empty())
}

fn position(table: &[&str], name: &str) -> Option<i32> {
    table.iter().position(|n| *n == name).map(|i| i as i32)
}
