//! The whitelist (`whitelist.json`).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{Shared, read, write};

/// One `whitelist.json` entry, in vanilla's serialization order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WhitelistRecord {
    pub(crate) uuid: Uuid,
    pub(crate) name: String,
}

/// The whitelist backing `whitelist.json`.
///
/// Name lookups are case-insensitive, matching how Minecraft treats player
/// names.
pub struct Whitelist {
    pub(crate) shared: Arc<Shared>,
}

impl Whitelist {
    /// Adds a player, or updates the name of an existing entry for `uuid`.
    pub fn add(&self, uuid: Uuid, name: &str) {
        let mut state = write(&self.shared.state);
        match state.whitelist.iter_mut().find(|entry| entry.uuid == uuid) {
            Some(existing) => existing.name = name.to_owned(),
            None => state.whitelist.push(WhitelistRecord {
                uuid,
                name: name.to_owned(),
            }),
        }
        state.dirty.whitelist = true;
    }

    /// Removes `uuid` from the whitelist, returning whether it was present.
    pub fn remove(&self, uuid: Uuid) -> bool {
        let mut state = write(&self.shared.state);
        let before = state.whitelist.len();
        state.whitelist.retain(|entry| entry.uuid != uuid);
        let removed = state.whitelist.len() != before;
        if removed {
            state.dirty.whitelist = true;
        }
        removed
    }

    /// Removes the entry whose name matches case-insensitively.
    ///
    /// Returns whether an entry was removed.
    pub fn remove_name(&self, name: &str) -> bool {
        let mut state = write(&self.shared.state);
        let before = state.whitelist.len();
        state
            .whitelist
            .retain(|entry| !entry.name.eq_ignore_ascii_case(name));
        let removed = state.whitelist.len() != before;
        if removed {
            state.dirty.whitelist = true;
        }
        removed
    }

    /// Every whitelisted name, sorted case-insensitively.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = read(&self.shared.state)
            .whitelist
            .iter()
            .map(|entry| entry.name.clone())
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names
    }

    /// Whether `uuid` is whitelisted.
    pub fn contains(&self, uuid: Uuid) -> bool {
        read(&self.shared.state)
            .whitelist
            .iter()
            .any(|entry| entry.uuid == uuid)
    }

    /// Whether a whitelisted entry has this name, ignoring case.
    pub fn contains_name(&self, name: &str) -> bool {
        read(&self.shared.state)
            .whitelist
            .iter()
            .any(|entry| entry.name.eq_ignore_ascii_case(name))
    }
}
