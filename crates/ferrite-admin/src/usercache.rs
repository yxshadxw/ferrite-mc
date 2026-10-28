//! The name/uuid profile cache (`usercache.json`).
//!
//! Vanilla writes this file differently from the other admin lists: 26.3
//! serializes it as one compact line in `uuid`, `name`, `expiresOn` order (the
//! other files are two-space pretty-printed). The serializer here matches that
//! byte for byte so a real server's cache round-trips unchanged.

use std::sync::Arc;

use chrono::Duration;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{Shared, read, write};
use crate::timestamp::Timestamp;

/// Vanilla keeps a cached profile for eight weeks.
const CACHE_EXPIRY_DAYS: i64 = 56;

/// One `usercache.json` entry, in vanilla's serialization order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CachedUser {
    pub(crate) uuid: Uuid,
    pub(crate) name: String,
    #[serde(rename = "expiresOn")]
    pub(crate) expires_on: Timestamp,
}

impl CachedUser {
    fn is_expired(&self, now: Timestamp) -> bool {
        now > self.expires_on
    }
}

/// The profile cache backing `usercache.json`.
///
/// Entries expire eight weeks after they were cached. Lookups ignore expired
/// entries and prune them lazily, matching the ban lists.
pub struct UserCache {
    pub(crate) shared: Arc<Shared>,
}

impl UserCache {
    /// Uuid cached for `name`, ignoring ASCII case.
    pub fn uuid_for(&self, name: &str) -> Option<Uuid> {
        self.prune_expired();
        read(&self.shared.state)
            .users
            .iter()
            .find(|user| user.name.eq_ignore_ascii_case(name))
            .map(|user| user.uuid)
    }

    /// Name cached for `uuid`.
    pub fn name_for(&self, uuid: Uuid) -> Option<String> {
        self.prune_expired();
        read(&self.shared.state)
            .users
            .iter()
            .find(|user| user.uuid == uuid)
            .map(|user| user.name.clone())
    }

    /// Caches `name`/`uuid` for eight weeks.
    ///
    /// Any existing entry with the same uuid or the same case-insensitive name
    /// is replaced, so the cache never holds two names for one player.
    pub fn cache(&self, name: &str, uuid: Uuid) {
        let now = self.shared.now();
        let mut state = write(&self.shared.state);
        state.users.retain(|user| {
            !user.is_expired(now) && user.uuid != uuid && !user.name.eq_ignore_ascii_case(name)
        });
        state.users.push(CachedUser {
            uuid,
            name: name.to_owned(),
            expires_on: now.after(Duration::days(CACHE_EXPIRY_DAYS)),
        });
        state.dirty.usercache = true;
    }

    fn prune_expired(&self) {
        let now = self.shared.now();
        let has_expired = read(&self.shared.state)
            .users
            .iter()
            .any(|user| user.is_expired(now));
        if !has_expired {
            return;
        }
        let mut state = write(&self.shared.state);
        let before = state.users.len();
        state.users.retain(|user| !user.is_expired(now));
        if state.users.len() != before {
            state.dirty.usercache = true;
        }
    }
}
