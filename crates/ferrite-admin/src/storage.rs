//! Shared state, JSON helpers and atomic file replacement.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Error;
use crate::bans::{IpBan, PlayerBan};
use crate::ops::OpRecord;
use crate::timestamp::{Clock, Timestamp};
use crate::usercache::CachedUser;
use crate::whitelist::WhitelistRecord;

/// File names used by a vanilla server.
pub(crate) const OPS_FILE: &str = "ops.json";
pub(crate) const WHITELIST_FILE: &str = "whitelist.json";
pub(crate) const BANNED_PLAYERS_FILE: &str = "banned-players.json";
pub(crate) const BANNED_IPS_FILE: &str = "banned-ips.json";
pub(crate) const USERCACHE_FILE: &str = "usercache.json";

/// State shared by [`AdminLists`](crate::AdminLists) and its sub-handles.
pub(crate) struct Shared {
    pub(crate) state: RwLock<State>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) save: Mutex<()>,
}

impl Shared {
    pub(crate) fn now(&self) -> Timestamp {
        self.clock.now()
    }
}

/// Everything persisted across the five files.
#[derive(Default)]
pub(crate) struct State {
    pub(crate) ops: Vec<OpRecord>,
    pub(crate) whitelist: Vec<WhitelistRecord>,
    pub(crate) banned_players: Vec<PlayerBan>,
    pub(crate) banned_ips: Vec<IpBan>,
    pub(crate) users: Vec<CachedUser>,
    pub(crate) dirty: Dirty,
}

/// Per-file dirty flags, so `flush` rewrites only what changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Dirty {
    pub(crate) ops: bool,
    pub(crate) whitelist: bool,
    pub(crate) banned_players: bool,
    pub(crate) banned_ips: bool,
    pub(crate) usercache: bool,
}

impl Dirty {
    pub(crate) fn is_clean(self) -> bool {
        self == Self::default()
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.ops |= other.ops;
        self.whitelist |= other.whitelist;
        self.banned_players |= other.banned_players;
        self.banned_ips |= other.banned_ips;
        self.usercache |= other.usercache;
    }
}

/// Acquires a read lock, recovering from a poisoned writer.
pub(crate) fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

/// Acquires a write lock, recovering from a poisoned writer.
pub(crate) fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

/// Acquires a mutex, recovering from a poisoned owner.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Reads a JSON array, treating a missing file as an empty list.
pub(crate) fn read_list<T: DeserializeOwned>(dir: &Path, file: &str) -> Result<Vec<T>, Error> {
    let path = dir.join(file);
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|source| Error::Parse { path, source }),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(Error::Read { path, source }),
    }
}

/// Serializes a value the way vanilla writes `ops.json` and the ban lists.
pub(crate) fn serialize_pretty<T: Serialize>(
    dir: &Path,
    file: &str,
    value: &T,
) -> Result<String, Error> {
    serde_json::to_string_pretty(value).map_err(|source| Error::Serialize {
        path: dir.join(file),
        source,
    })
}

/// Serializes a value the way vanilla writes `usercache.json`.
pub(crate) fn serialize_compact<T: Serialize>(
    dir: &Path,
    file: &str,
    value: &T,
) -> Result<String, Error> {
    serde_json::to_string(value).map_err(|source| Error::Serialize {
        path: dir.join(file),
        source,
    })
}

/// Replaces `path` atomically by writing `path.tmp` and renaming it.
///
/// A crash can leave the temporary file behind; [`read_list`] never looks at
/// it, and the next save overwrites it.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<(), Error> {
    let temp_path = temp_path(path);
    let mut file = fs::File::create(&temp_path).map_err(|source| Error::Write {
        path: temp_path.clone(),
        source,
    })?;
    if let Err(source) = file.write_all(contents.as_bytes()) {
        return Err(Error::Write {
            path: temp_path,
            source,
        });
    }
    if let Err(source) = file.sync_all() {
        return Err(Error::Write {
            path: temp_path,
            source,
        });
    }
    drop(file);
    fs::rename(&temp_path, path).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn temp_path(path: &Path) -> PathBuf {
    match path.file_name() {
        Some(name) => {
            let mut name = name.to_os_string();
            name.push(".tmp");
            path.with_file_name(name)
        }
        None => path.with_extension("tmp"),
    }
}
