//! Item tags, including nested `#tag` references.
//!
//! Loot data uses item tags for `minecraft:tag` entries and for `#tag`
//! references inside `match_tool` item predicates. Tags are resolved once at
//! load time so evaluation never touches the filesystem.

use crate::files::{json_files, read_json, relative_stem};
use crate::{Result, normalize_id};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Resolved item tags for one namespace.
#[derive(Debug, Default)]
pub(crate) struct ItemTags {
    /// Sorted, de-duplicated item ids per full tag id.
    sets: BTreeMap<String, Vec<String>>,
}

impl ItemTags {
    /// Loads every tag below `dir/**/*.json`.
    ///
    /// A missing directory yields no tags (and therefore no tag drops); every
    /// tag that is present is resolved, skipping nested references this pack
    /// does not define.
    pub(crate) fn load(dir: &Path, namespace: &str) -> Result<Self> {
        if !dir.is_dir() {
            return Ok(Self::default());
        }

        let mut raw: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for path in json_files(dir)? {
            let Some(relative) = relative_stem(&path, dir) else {
                continue;
            };
            let value = read_json(&path)?;
            let Some(values) = value.get("values").and_then(Value::as_array) else {
                continue;
            };
            let mut entries = Vec::with_capacity(values.len());
            for value in values {
                match value {
                    Value::String(id) => entries.push(id.clone()),
                    Value::Object(object) => {
                        if let Some(id) = object.get("id").and_then(Value::as_str) {
                            entries.push(id.to_owned());
                        }
                    }
                    _ => {}
                }
            }
            let name = format!("{namespace}:{relative}");
            raw.insert(name, entries);
        }

        Ok(Self {
            sets: resolve(&raw),
        })
    }

    /// The resolved items of a tag, or `None` when the tag is unknown.
    pub(crate) fn items(&self, tag: &str) -> Option<&[String]> {
        let tag = normalize_id(tag);
        self.sets.get(&tag).map(Vec::as_slice)
    }

    /// Number of loaded tags.
    pub(crate) fn len(&self) -> usize {
        self.sets.len()
    }
}

fn resolve(raw: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, Vec<String>> {
    let mut sets = BTreeMap::new();
    let mut state = BTreeMap::new();
    for name in raw.keys() {
        resolve_tag(name, raw, &mut sets, &mut state);
    }
    sets
}

fn resolve_tag(
    name: &str,
    raw: &BTreeMap<String, Vec<String>>,
    sets: &mut BTreeMap<String, Vec<String>>,
    state: &mut BTreeMap<String, u8>,
) -> Vec<String> {
    match state.get(name) {
        // Already resolved.
        Some(2) => return sets.get(name).cloned().unwrap_or_default(),
        // Cyclic tag: treat it as empty instead of failing the whole pack.
        Some(_) => return Vec::new(),
        None => {}
    }

    state.insert(name.to_owned(), 1);
    let mut items = BTreeSet::new();
    for entry in raw.get(name).map(Vec::as_slice).unwrap_or(&[]) {
        if let Some(tag) = entry.strip_prefix('#') {
            let tag = normalize_id(tag);
            if raw.contains_key(&tag) {
                items.extend(resolve_tag(&tag, raw, sets, state));
            }
        } else {
            items.insert(normalize_id(entry));
        }
    }
    state.insert(name.to_owned(), 2);

    let items: Vec<String> = items.into_iter().collect();
    sets.insert(name.to_owned(), items.clone());
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(entries: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        entries
            .iter()
            .map(|(name, values)| {
                (
                    (*name).to_owned(),
                    values.iter().map(|value| (*value).to_owned()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn nested_tags_are_resolved() {
        let tags = resolve(&raw(&[
            ("minecraft:base", &["minecraft:oak_planks"]),
            ("minecraft:parent", &["#minecraft:base", "minecraft:stick"]),
        ]));
        assert_eq!(
            tags["minecraft:parent"],
            ["minecraft:oak_planks", "minecraft:stick"]
        );
    }

    #[test]
    fn missing_nested_tags_are_skipped() {
        let tags = resolve(&raw(&[(
            "minecraft:test",
            &["#minecraft:missing", "stick"],
        )]));
        assert_eq!(tags["minecraft:test"], ["minecraft:stick"]);
    }

    #[test]
    fn cycles_do_not_loop_forever() {
        let tags = resolve(&raw(&[
            ("minecraft:a", &["#minecraft:b"]),
            ("minecraft:b", &["#minecraft:a", "minecraft:stick"]),
        ]));
        assert_eq!(tags["minecraft:b"], ["minecraft:stick"]);
    }

    #[test]
    fn duplicates_are_removed() {
        let tags = resolve(&raw(&[("minecraft:test", &["stick", "minecraft:stick"])]));
        assert_eq!(tags["minecraft:test"], ["minecraft:stick"]);
    }
}
