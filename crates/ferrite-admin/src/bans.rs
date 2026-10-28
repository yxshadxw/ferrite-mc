//! Player and IP bans (`banned-players.json`, `banned-ips.json`).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage::{Shared, read, write};
use crate::timestamp::Timestamp;

/// A player ban entry from `banned-players.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerBan {
    /// Banned player.
    pub uuid: Uuid,
    /// Name at the time of the ban.
    pub name: String,
    /// When the ban was created.
    pub created: Timestamp,
    /// Who issued the ban (`Server` for console commands).
    pub source: String,
    /// Expiry, or `None` for a permanent ban (`"forever"` on disk).
    #[serde(with = "expiry")]
    pub expires: Option<Timestamp>,
    /// Human-readable reason.
    pub reason: String,
}

/// An IP ban entry from `banned-ips.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpBan {
    /// Banned address, exactly as stored in the file.
    pub ip: String,
    /// When the ban was created.
    pub created: Timestamp,
    /// Who issued the ban (`Server` for console commands).
    pub source: String,
    /// Expiry, or `None` for a permanent ban (`"forever"` on disk).
    #[serde(with = "expiry")]
    pub expires: Option<Timestamp>,
    /// Human-readable reason.
    pub reason: String,
}

impl PlayerBan {
    fn is_expired(&self, now: Timestamp) -> bool {
        self.expires.is_some_and(|expires| now > expires)
    }
}

impl IpBan {
    fn is_expired(&self, now: Timestamp) -> bool {
        self.expires.is_some_and(|expires| now > expires)
    }
}

/// The player and IP ban lists.
///
/// Expiry is evaluated against the injected [`Clock`](crate::Clock). Expired
/// entries are pruned lazily: the first query that sees one removes all
/// expired entries and marks the file dirty.
pub struct Bans {
    pub(crate) shared: Arc<Shared>,
}

impl Bans {
    /// Whether `uuid` is currently banned (expired entries do not count).
    pub fn is_banned(&self, uuid: Uuid) -> bool {
        self.prune_players();
        read(&self.shared.state)
            .banned_players
            .iter()
            .any(|ban| ban.uuid == uuid)
    }

    /// Whether `ip` is currently banned, ignoring ASCII case.
    pub fn is_ip_banned(&self, ip: &str) -> bool {
        self.prune_ips();
        read(&self.shared.state)
            .banned_ips
            .iter()
            .any(|ban| ban.ip.eq_ignore_ascii_case(ip))
    }

    /// The active ban for `uuid`, including its reason and source.
    pub fn player_ban(&self, uuid: Uuid) -> Option<PlayerBan> {
        self.prune_players();
        read(&self.shared.state)
            .banned_players
            .iter()
            .find(|ban| ban.uuid == uuid)
            .cloned()
    }

    /// The active ban for `ip`, ignoring ASCII case.
    pub fn ip_ban(&self, ip: &str) -> Option<IpBan> {
        self.prune_ips();
        read(&self.shared.state)
            .banned_ips
            .iter()
            .find(|ban| ban.ip.eq_ignore_ascii_case(ip))
            .cloned()
    }

    /// Snapshot of every active player ban.
    pub fn player_bans(&self) -> Vec<PlayerBan> {
        self.prune_players();
        read(&self.shared.state).banned_players.clone()
    }

    /// Snapshot of every active IP ban.
    pub fn ip_bans(&self) -> Vec<IpBan> {
        self.prune_ips();
        read(&self.shared.state).banned_ips.clone()
    }

    /// Bans a player, replacing an existing entry for the same uuid.
    ///
    /// `expires` of `None` writes `"forever"`.
    pub fn ban(
        &self,
        uuid: Uuid,
        name: &str,
        reason: &str,
        source: &str,
        expires: Option<Timestamp>,
    ) {
        let record = PlayerBan {
            uuid,
            name: name.to_owned(),
            created: self.shared.now(),
            source: source.to_owned(),
            expires,
            reason: reason.to_owned(),
        };
        let mut state = write(&self.shared.state);
        match state.banned_players.iter_mut().find(|ban| ban.uuid == uuid) {
            Some(existing) => *existing = record,
            None => state.banned_players.push(record),
        }
        state.dirty.banned_players = true;
    }

    /// Lifts a player ban, returning whether one existed.
    pub fn unban(&self, uuid: Uuid) -> bool {
        let mut state = write(&self.shared.state);
        let before = state.banned_players.len();
        state.banned_players.retain(|ban| ban.uuid != uuid);
        let removed = state.banned_players.len() != before;
        if removed {
            state.dirty.banned_players = true;
        }
        removed
    }

    /// Bans an IP address, replacing an existing entry that matches it
    /// case-insensitively.
    ///
    /// `expires` of `None` writes `"forever"`.
    pub fn ban_ip(&self, ip: &str, reason: &str, source: &str, expires: Option<Timestamp>) {
        let record = IpBan {
            ip: ip.to_owned(),
            created: self.shared.now(),
            source: source.to_owned(),
            expires,
            reason: reason.to_owned(),
        };
        let mut state = write(&self.shared.state);
        match state
            .banned_ips
            .iter_mut()
            .find(|ban| ban.ip.eq_ignore_ascii_case(ip))
        {
            Some(existing) => *existing = record,
            None => state.banned_ips.push(record),
        }
        state.dirty.banned_ips = true;
    }

    /// Lifts an IP ban, returning whether one existed.
    pub fn unban_ip(&self, ip: &str) -> bool {
        let mut state = write(&self.shared.state);
        let before = state.banned_ips.len();
        state
            .banned_ips
            .retain(|ban| !ban.ip.eq_ignore_ascii_case(ip));
        let removed = state.banned_ips.len() != before;
        if removed {
            state.dirty.banned_ips = true;
        }
        removed
    }

    fn prune_players(&self) {
        let now = self.shared.now();
        let has_expired = read(&self.shared.state)
            .banned_players
            .iter()
            .any(|ban| ban.is_expired(now));
        if !has_expired {
            return;
        }
        let mut state = write(&self.shared.state);
        let before = state.banned_players.len();
        state.banned_players.retain(|ban| !ban.is_expired(now));
        if state.banned_players.len() != before {
            state.dirty.banned_players = true;
        }
    }

    fn prune_ips(&self) {
        let now = self.shared.now();
        let has_expired = read(&self.shared.state)
            .banned_ips
            .iter()
            .any(|ban| ban.is_expired(now));
        if !has_expired {
            return;
        }
        let mut state = write(&self.shared.state);
        let before = state.banned_ips.len();
        state.banned_ips.retain(|ban| !ban.is_expired(now));
        if state.banned_ips.len() != before {
            state.dirty.banned_ips = true;
        }
    }
}

/// Serde adapter for vanilla's `expires` field: a timestamp or `"forever"`.
mod expiry {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::timestamp::Timestamp;

    pub(crate) fn serialize<S: Serializer>(
        expires: &Option<Timestamp>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match expires {
            Some(timestamp) => serializer.collect_str(timestamp),
            None => serializer.serialize_str("forever"),
        }
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Timestamp>, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value == "forever" {
            return Ok(None);
        }
        Timestamp::parse(&value)
            .map(Some)
            .map_err(serde::de::Error::custom)
    }
}
