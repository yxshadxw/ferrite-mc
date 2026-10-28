//! Item tags, including nested `#tag` references and optional entries.

use crate::files::{json_files, read_json, relative_stem};
use crate::recipe::{normalize_id, normalize_tag};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Resolved item tags for one namespace.
#[derive(Debug, Clone, Default)]
pub struct ItemTags {
    sets: HashMap<String, HashSet<String>>,
    by_item: HashMap<String, Vec<String>>,
}

impl ItemTags {
    /// Loads every tag below `<tags_dir>/**/*.json`.
    pub(crate) fn load(tags_dir: &Path, namespace: &str) -> Result<Self> {
        let mut raw = HashMap::new();
        for path in json_files(tags_dir)? {
            let value = read_json(&path)?;
            let relative = relative_stem(&path, tags_dir)
                .ok_or_else(|| Error::tag("?", "tag path is outside the tags directory"))?;
            let name = format!("{namespace}:{relative}");
            let object = value
                .as_object()
                .ok_or_else(|| Error::tag(&name, "a tag file must be a JSON object"))?;
            let values: &[Value] = match object.get("values") {
                None => &[],
                Some(Value::Array(values)) => values,
                Some(_) => {
                    return Err(Error::tag(&name, "`values` must be an array"));
                }
            };
            let entries = values
                .iter()
                .map(|value| TagEntry::from_value(value, &name))
                .collect::<Result<Vec<_>>>()?;
            if raw.insert(name.clone(), entries).is_some() {
                return Err(Error::tag(&name, "duplicate tag file"));
            }
        }
        Self::resolve(raw)
    }

    fn resolve(raw: HashMap<String, Vec<TagEntry>>) -> Result<Self> {
        let mut sets = HashMap::with_capacity(raw.len());
        let mut state = HashMap::with_capacity(raw.len());
        let mut names: Vec<&String> = raw.keys().collect();
        names.sort();
        for name in names {
            resolve_tag(name, &raw, &mut sets, &mut state)?;
        }

        let mut by_item: HashMap<String, Vec<String>> = HashMap::new();
        for (tag, items) in &sets {
            for item in items {
                by_item.entry(item.clone()).or_default().push(tag.clone());
            }
        }
        for tags in by_item.values_mut() {
            tags.sort();
            tags.dedup();
        }

        Ok(Self { sets, by_item })
    }

    /// Whether `tag` contains `item`. Both names may omit `minecraft:`.
    pub fn contains(&self, tag: &str, item: &str) -> bool {
        let tag = normalize_tag(tag);
        let item = normalize_id(item);
        self.sets
            .get(tag.as_ref())
            .is_some_and(|items| items.contains(item.as_ref()))
    }

    /// The resolved items of a tag, or `None` when the tag is unknown.
    pub fn items(&self, tag: &str) -> Option<&HashSet<String>> {
        let tag = normalize_tag(tag);
        self.sets.get(tag.as_ref())
    }

    /// Every tag containing an item.
    pub fn tags_for_item(&self, item: &str) -> &[String] {
        let item = normalize_id(item);
        self.by_item
            .get(item.as_ref())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Number of loaded tags.
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// Whether no tags were loaded.
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// Loaded tag names.
    pub fn tag_names(&self) -> impl Iterator<Item = &str> {
        self.sets.keys().map(String::as_str)
    }
}

#[derive(Debug, Clone)]
struct TagEntry {
    id: String,
    required: bool,
}

impl TagEntry {
    fn from_value(value: &Value, tag: &str) -> Result<Self> {
        match value {
            Value::String(id) => {
                if id.is_empty() {
                    return Err(Error::tag(tag, "a tag entry must not be empty"));
                }
                Ok(Self {
                    id: id.clone(),
                    required: true,
                })
            }
            Value::Object(object) => {
                let id = object
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::tag(tag, "a tag entry object needs an `id` string"))?;
                if id.is_empty() {
                    return Err(Error::tag(tag, "a tag entry must not be empty"));
                }
                let required = match object.get("required") {
                    None => true,
                    Some(Value::Bool(required)) => *required,
                    Some(_) => {
                        return Err(Error::tag(tag, "a tag entry `required` must be a boolean"));
                    }
                };
                Ok(Self {
                    id: id.to_owned(),
                    required,
                })
            }
            _ => Err(Error::tag(
                tag,
                "a tag entry must be a string or an object with an `id`",
            )),
        }
    }
}

fn resolve_tag(
    name: &str,
    raw: &HashMap<String, Vec<TagEntry>>,
    sets: &mut HashMap<String, HashSet<String>>,
    state: &mut HashMap<String, u8>,
) -> Result<()> {
    match state.get(name).copied() {
        Some(2) => return Ok(()),
        Some(1) => return Err(Error::tag(name, "cyclic tag reference")),
        _ => {}
    }

    let entries = raw
        .get(name)
        .cloned()
        .ok_or_else(|| Error::tag(name, "tag is not defined"))?;
    state.insert(name.to_owned(), 1);

    let mut items = HashSet::new();
    for entry in entries {
        if entry.id.starts_with('#') {
            let referenced = normalize_tag(&entry.id).into_owned();
            if raw.contains_key(&referenced) {
                resolve_tag(&referenced, raw, sets, state)?;
                if let Some(referenced_items) = sets.get(&referenced) {
                    items.extend(referenced_items.iter().cloned());
                }
            } else if entry.required {
                return Err(Error::tag(
                    name,
                    format!("required tag `{referenced}` is not defined"),
                ));
            }
        } else {
            items.insert(normalize_id(&entry.id).into_owned());
        }
    }

    state.insert(name.to_owned(), 2);
    sets.insert(name.to_owned(), items);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, required: bool) -> TagEntry {
        TagEntry {
            id: id.to_owned(),
            required,
        }
    }

    #[test]
    fn nested_tags_are_resolved() {
        let mut raw = HashMap::new();
        raw.insert(
            "minecraft:base".to_owned(),
            vec![entry("minecraft:oak_planks", true)],
        );
        raw.insert(
            "minecraft:parent".to_owned(),
            vec![
                entry("#minecraft:base", true),
                entry("minecraft:stick", true),
            ],
        );
        let tags = ItemTags::resolve(raw).unwrap();
        assert!(tags.contains("minecraft:parent", "oak_planks"));
        assert!(tags.contains("minecraft:parent", "minecraft:stick"));
        assert!(!tags.contains("minecraft:parent", "minecraft:dirt"));
        assert_eq!(
            tags.tags_for_item("minecraft:oak_planks"),
            ["minecraft:base".to_owned(), "minecraft:parent".to_owned()].as_slice()
        );
    }

    #[test]
    fn optional_missing_tags_are_skipped() {
        let mut raw = HashMap::new();
        raw.insert(
            "minecraft:test".to_owned(),
            vec![
                entry("minecraft:stick", true),
                entry("#minecraft:missing", false),
            ],
        );
        let tags = ItemTags::resolve(raw).unwrap();
        assert!(tags.contains("minecraft:test", "stick"));
        assert_eq!(tags.items("minecraft:test").unwrap().len(), 1);
    }

    #[test]
    fn missing_required_tags_are_errors() {
        let mut raw = HashMap::new();
        raw.insert(
            "minecraft:test".to_owned(),
            vec![entry("#minecraft:missing", true)],
        );
        assert!(ItemTags::resolve(raw).is_err());
    }

    #[test]
    fn cycles_are_errors() {
        let mut raw = HashMap::new();
        raw.insert("minecraft:a".to_owned(), vec![entry("#minecraft:b", true)]);
        raw.insert("minecraft:b".to_owned(), vec![entry("#minecraft:a", true)]);
        assert!(ItemTags::resolve(raw).is_err());
    }

    #[test]
    fn object_entries_parse_required_flag() {
        let value = serde_json::json!({"id": "#minecraft:missing", "required": false});
        let entry = TagEntry::from_value(&value, "minecraft:test").unwrap();
        assert!(!entry.required);
        assert_eq!(entry.id, "#minecraft:missing");

        let value = serde_json::json!({"id": "minecraft:stick"});
        assert!(
            TagEntry::from_value(&value, "minecraft:test")
                .unwrap()
                .required
        );
    }
}
