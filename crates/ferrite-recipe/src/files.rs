//! Filesystem helpers for loading generated data packs.

use crate::{Error, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Collects the `.json` files directly inside `dir`, in sorted order.
///
/// Vanilla 26.3 keeps its 1,763 top-level recipes in `recipe/` and puts the
/// separate brewing recipes in a subdirectory, which is not part of the
/// crafting recipe system loaded here.
pub(crate) fn json_files_flat(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Err(Error::Io {
            path: dir.display().to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "directory does not exist"),
        });
    }

    let mut files = Vec::new();
    let entries = fs::read_dir(dir).map_err(|error| Error::io(dir, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| Error::io(dir, error))?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| Error::io(&path, error))?;
        if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "json")
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Collects every `.json` file below `dir`, recursively, in sorted order.
pub(crate) fn json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Err(Error::Io {
            path: dir.display().to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "directory does not exist"),
        });
    }

    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory).map_err(|error| Error::io(&directory, error))?;
        for entry in entries {
            let entry = entry.map_err(|error| Error::io(&directory, error))?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|error| Error::io(&path, error))?;
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "json")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Reads and parses one JSON file with path-aware errors.
pub(crate) fn read_json(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).map_err(|error| Error::io(path, error))?;
    serde_json::from_slice(&bytes).map_err(|error| Error::json(path, error))
}

/// A file's path below `base`, without the extension and with `/` separators.
pub(crate) fn relative_stem(path: &Path, base: &Path) -> Option<String> {
    let relative = path.strip_prefix(base).ok()?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let part = component.as_os_str().to_string_lossy();
        if part.is_empty() {
            continue;
        }
        parts.push(part.into_owned());
    }
    let last = parts.last_mut()?;
    if let Some(stem) = last.strip_suffix(".json") {
        *last = stem.to_owned();
    }
    Some(parts.join("/"))
}
