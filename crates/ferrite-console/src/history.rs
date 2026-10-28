//! Persistent command history.
//!
//! Entries are kept newest-last in memory, capped at [`DEFAULT_HISTORY_LIMIT`],
//! de-duplicated against the previous entry, and written back as a plain text
//! file (one command per line) after every accepted command.

use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Maximum number of remembered commands.
pub const DEFAULT_HISTORY_LIMIT: usize = 1000;

/// A bounded command history, optionally backed by a file.
#[derive(Debug)]
pub struct History {
    entries: VecDeque<String>,
    limit: usize,
    path: Option<PathBuf>,
}

impl History {
    /// In-memory history with the given cap. A cap of zero disables history.
    pub fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            limit,
            path: None,
        }
    }

    /// Load history from `path`. A missing file is an empty history.
    pub fn load(path: impl AsRef<Path>, limit: usize) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut history = Self {
            entries: VecDeque::new(),
            limit,
            path: Some(path.clone()),
        };
        match fs::read_to_string(&path) {
            Ok(contents) => {
                for line in contents.lines() {
                    let line = line.trim();
                    if !line.is_empty() {
                        history.push_entry(line.to_string());
                    }
                }
                Ok(history)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(history),
            Err(error) => Err(error),
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entry counted backwards from the newest one (`0` is the latest command).
    pub fn get(&self, from_newest: usize) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .nth(from_newest)
            .map(String::as_str)
    }

    /// Record a submitted command. Blank lines and repeats of the previous
    /// command are ignored. Persists immediately when a path is configured.
    pub fn record(&mut self, line: &str) -> io::Result<()> {
        let line = line.trim();
        if line.is_empty() || self.limit == 0 {
            return Ok(());
        }
        if self.entries.back().is_some_and(|last| last == line) {
            return Ok(());
        }
        self.push_entry(line.to_string());
        self.save()
    }

    /// Rewrite the backing file, atomically when possible.
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let mut contents = String::new();
        for entry in &self.entries {
            contents.push_str(entry);
            contents.push('\n');
        }
        let temporary = path.with_file_name(format!(
            "{}.tmp",
            path.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default()
        ));
        fs::write(&temporary, contents)?;
        fs::rename(temporary, path)
    }

    fn push_entry(&mut self, entry: String) {
        if self.limit == 0 {
            return;
        }
        self.entries.push_back(entry);
        while self.entries.len() > self.limit {
            self.entries.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ferrite-console-{name}-{}-{unique}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn record_dedupes_and_caps() {
        let mut history = History::new(3);
        history.record("one").unwrap();
        history.record("one").unwrap();
        history.record("two").unwrap();
        history.record("three").unwrap();
        history.record("four").unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history.get(0), Some("four"));
        assert_eq!(history.get(1), Some("three"));
        assert_eq!(history.get(2), Some("two"));
        assert_eq!(history.get(3), None);
        assert!(history.record("   ").is_ok());
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn persists_and_reloads() {
        let dir = TempDir::new("history");
        let path = dir.join("console-history.txt");
        {
            let mut history = History::load(&path, DEFAULT_HISTORY_LIMIT).unwrap();
            history.record("say hello").unwrap();
            history.record("list").unwrap();
        }
        let loaded = History::load(&path, DEFAULT_HISTORY_LIMIT).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get(0), Some("list"));
        assert_eq!(loaded.get(1), Some("say hello"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "say hello\nlist\n");
    }

    #[test]
    fn reload_keeps_newest_entries_within_limit() {
        let dir = TempDir::new("reload-limit");
        let path = dir.join("history.txt");
        fs::write(&path, "a\nb\nc\nd\ne\n").unwrap();
        let loaded = History::load(&path, 2).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get(0), Some("e"));
        assert_eq!(loaded.get(1), Some("d"));
    }

    #[test]
    fn missing_file_loads_empty() {
        let dir = TempDir::new("missing");
        let history = History::load(dir.join("nope.txt"), 10).unwrap();
        assert!(history.is_empty());
    }
}
