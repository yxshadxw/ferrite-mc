//! The NBT path syntax shared by `/data` and `execute store`: `a.b[0].c`.
//!
//! Only the parts of vanilla's path grammar commands can produce are supported: compound
//! keys and list indices, plus an empty path for the root tag. Key filters (`{...}`) and
//! quoted keys are rejected, which the command layer reports as the vanilla
//! `argument.nbt_path` errors.

use ferrite_nbt::{Compound, List, Tag};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Key(String),
    Index(usize),
}

/// A parsed NBT path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NbtPath {
    segments: Vec<Segment>,
    text: String,
}

impl NbtPath {
    /// Parses `input`; an empty path selects the root tag.
    pub fn parse(input: &str) -> Result<Self, String> {
        let text = input.trim();
        if text.is_empty() {
            return Ok(Self {
                segments: Vec::new(),
                text: String::new(),
            });
        }
        let bytes = text.as_bytes();
        let mut segments = Vec::new();
        let mut i = 0;
        loop {
            if i < bytes.len() && bytes[i] == b'[' {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                if end == start || bytes.get(end) != Some(&b']') {
                    return Err(format!("Invalid NBT path '{text}'"));
                }
                segments.push(Segment::Index(
                    text[start..end]
                        .parse()
                        .map_err(|_| format!("Invalid list index in path '{text}'"))?,
                ));
                i = end + 1;
            } else {
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                if i == start {
                    return Err(format!("Invalid NBT path '{text}'"));
                }
                segments.push(Segment::Key(text[start..i].to_owned()));
            }
            if i == bytes.len() {
                break;
            }
            if bytes[i] == b'.' {
                i += 1;
                if i == bytes.len() {
                    return Err(format!("Invalid NBT path '{text}'"));
                }
            } else if bytes[i] != b'[' {
                return Err(format!("Invalid NBT path '{text}'"));
            }
        }
        Ok(Self {
            segments,
            text: text.to_owned(),
        })
    }

    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// The path as typed.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The tag at this path, cloned; `None` when the path does not resolve.
    pub fn get(&self, root: &Tag) -> Option<Tag> {
        get_at(root, &self.segments)
    }

    /// Writes `value` at this path, creating missing compounds.
    pub fn set(&self, root: &mut Tag, value: Tag) -> Result<(), String> {
        set_at(root, &self.segments, value)
    }

    /// Merges `value` into the compound at this path (vanilla's `/data merge`).
    pub fn merge(&self, root: &mut Tag, value: &Tag) -> Result<bool, String> {
        let mut target = self.get(root).unwrap_or(Tag::Compound(Compound::new()));
        let changed = merge_tags(&mut target, value);
        if changed {
            self.set(root, target)?;
        }
        Ok(changed)
    }

    /// Removes and returns the tag at this path.
    pub fn remove(&self, root: &mut Tag) -> Result<Tag, String> {
        remove_at(root, &self.segments)
    }
}

fn get_at(tag: &Tag, segments: &[Segment]) -> Option<Tag> {
    let Some((first, rest)) = segments.split_first() else {
        return Some(tag.clone());
    };
    match (first, tag) {
        (Segment::Key(key), Tag::Compound(map)) => get_at(map.get(key)?, rest),
        (Segment::Index(index), Tag::List(list)) => get_at(&get_index(list, *index)?, rest),
        _ => None,
    }
}

fn get_index(list: &List, index: usize) -> Option<Tag> {
    Some(match list {
        List::Byte(v) => Tag::Byte(*v.get(index)?),
        List::Short(v) => Tag::Short(*v.get(index)?),
        List::Int(v) => Tag::Int(*v.get(index)?),
        List::Long(v) => Tag::Long(*v.get(index)?),
        List::Float(v) => Tag::Float(*v.get(index)?),
        List::Double(v) => Tag::Double(*v.get(index)?),
        List::String(v) => Tag::String(v.get(index)?.clone()),
        List::List(v) => Tag::List(v.get(index)?.clone()),
        List::Compound(v) => Tag::Compound(v.get(index)?.clone()),
        List::ByteArray(v) => Tag::ByteArray(v.get(index)?.clone()),
        List::IntArray(v) => Tag::IntArray(v.get(index)?.clone()),
        List::LongArray(v) => Tag::LongArray(v.get(index)?.clone()),
        List::End => return None,
    })
}

fn set_at(tag: &mut Tag, segments: &[Segment], value: Tag) -> Result<(), String> {
    let Some((first, rest)) = segments.split_first() else {
        *tag = value;
        return Ok(());
    };
    match first {
        Segment::Key(key) => {
            if !matches!(tag, Tag::Compound(_)) {
                *tag = Tag::Compound(Compound::new());
            }
            let Tag::Compound(map) = tag else {
                unreachable!()
            };
            let entry = map
                .entry(key.clone())
                .or_insert(Tag::Compound(Compound::new()));
            set_at(entry, rest, value)
        }
        Segment::Index(index) => {
            let Tag::List(list) = tag else {
                return Err("Expected a list".into());
            };
            set_index(list, *index, value)
        }
    }
}

fn set_index(list: &mut List, index: usize, value: Tag) -> Result<(), String> {
    let invalid = || format!("Invalid list index: {index}");
    match (list, value) {
        (List::Compound(v), Tag::Compound(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::List(v), Tag::List(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::String(v), Tag::String(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::Byte(v), Tag::Byte(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::Short(v), Tag::Short(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::Int(v), Tag::Int(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::Long(v), Tag::Long(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::Float(v), Tag::Float(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::Double(v), Tag::Double(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::ByteArray(v), Tag::ByteArray(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::IntArray(v), Tag::IntArray(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        (List::LongArray(v), Tag::LongArray(x)) => *v.get_mut(index).ok_or_else(invalid)? = x,
        _ => return Err("Can't insert a tag of a different type into this list".into()),
    }
    Ok(())
}

fn merge_tags(target: &mut Tag, value: &Tag) -> bool {
    match (target, value) {
        (Tag::Compound(map), Tag::Compound(other)) => {
            let mut changed = false;
            for (key, value) in other {
                match map.get_mut(key) {
                    Some(existing) => changed |= merge_tags(existing, value),
                    None => {
                        map.insert(key.clone(), value.clone());
                        changed = true;
                    }
                }
            }
            changed
        }
        (existing, value) => {
            let changed = existing != value;
            if changed {
                *existing = value.clone();
            }
            changed
        }
    }
}

fn remove_at(tag: &mut Tag, segments: &[Segment]) -> Result<Tag, String> {
    let Some((first, rest)) = segments.split_first() else {
        let taken = tag.clone();
        *tag = Tag::Compound(Compound::new());
        return Ok(taken);
    };
    match first {
        Segment::Key(key) => {
            let Tag::Compound(map) = tag else {
                return Err("Can't get the path; tag doesn't exist".into());
            };
            if rest.is_empty() {
                return map
                    .shift_remove(key)
                    .ok_or_else(|| "Can't get the path; tag doesn't exist".to_owned());
            }
            let entry = map
                .get_mut(key)
                .ok_or_else(|| "Can't get the path; tag doesn't exist".to_owned())?;
            remove_at(entry, rest)
        }
        Segment::Index(index) => {
            let Tag::List(list) = tag else {
                return Err("Expected a list".into());
            };
            let removed =
                get_index(list, *index).ok_or_else(|| format!("Invalid list index: {index}"))?;
            remove_index(list, *index);
            Ok(removed)
        }
    }
}

fn remove_index(list: &mut List, index: usize) {
    match list {
        List::Byte(v) => drop(v.remove(index)),
        List::Short(v) => drop(v.remove(index)),
        List::Int(v) => drop(v.remove(index)),
        List::Long(v) => drop(v.remove(index)),
        List::Float(v) => drop(v.remove(index)),
        List::Double(v) => drop(v.remove(index)),
        List::String(v) => drop(v.remove(index)),
        List::List(v) => drop(v.remove(index)),
        List::Compound(v) => drop(v.remove(index)),
        List::ByteArray(v) => drop(v.remove(index)),
        List::IntArray(v) => drop(v.remove(index)),
        List::LongArray(v) => drop(v.remove(index)),
        List::End => {}
    }
}

/// Appends `value` to the list at `path` (vanilla's `modify ... append`).
pub fn append(path: &NbtPath, root: &mut Tag, value: Tag) -> Result<(), String> {
    let mut list = path.get(root).unwrap_or(Tag::List(List::End));
    match &mut list {
        Tag::List(list) => push(list, value)?,
        _ => return Err("Expected a list".into()),
    }
    path.set(root, list)
}

/// Prepends `value` to the list at `path` (vanilla's `modify ... prepend`).
pub fn prepend(path: &NbtPath, root: &mut Tag, value: Tag) -> Result<(), String> {
    let mut list = path.get(root).unwrap_or(Tag::List(List::End));
    match &mut list {
        Tag::List(list) => insert(list, 0, value)?,
        _ => return Err("Expected a list".into()),
    }
    path.set(root, list)
}

fn push(list: &mut List, value: Tag) -> Result<(), String> {
    let index = list_len(list);
    insert(list, index, value)
}

fn list_len(list: &List) -> usize {
    match list {
        List::Byte(v) => v.len(),
        List::Short(v) => v.len(),
        List::Int(v) => v.len(),
        List::Long(v) => v.len(),
        List::Float(v) => v.len(),
        List::Double(v) => v.len(),
        List::String(v) => v.len(),
        List::List(v) => v.len(),
        List::Compound(v) => v.len(),
        List::ByteArray(v) => v.len(),
        List::IntArray(v) => v.len(),
        List::LongArray(v) => v.len(),
        List::End => 0,
    }
}

fn insert(list: &mut List, index: usize, value: Tag) -> Result<(), String> {
    if matches!(list, List::End) {
        *list = match value {
            Tag::Compound(x) => List::Compound(vec![x]),
            Tag::String(x) => List::String(vec![x]),
            Tag::Int(x) => List::Int(vec![x]),
            Tag::Double(x) => List::Double(vec![x]),
            _ => return Err(format!("Can't insert into an empty list at index {index}")),
        };
        return Ok(());
    }
    let invalid = || format!("Invalid list index: {index}");
    match (list, value) {
        (List::Compound(v), Tag::Compound(x)) => v.insert(index, x),
        (List::List(v), Tag::List(x)) => v.insert(index, x),
        (List::String(v), Tag::String(x)) => v.insert(index, x),
        (List::Byte(v), Tag::Byte(x)) => v.insert(index, x),
        (List::Short(v), Tag::Short(x)) => v.insert(index, x),
        (List::Int(v), Tag::Int(x)) => v.insert(index, x),
        (List::Long(v), Tag::Long(x)) => v.insert(index, x),
        (List::Float(v), Tag::Float(x)) => v.insert(index, x),
        (List::Double(v), Tag::Double(x)) => v.insert(index, x),
        (List::ByteArray(v), Tag::ByteArray(x)) => v.insert(index, x),
        (List::IntArray(v), Tag::IntArray(x)) => v.insert(index, x),
        (List::LongArray(v), Tag::LongArray(x)) => v.insert(index, x),
        (List::End, _) => return Err(invalid()),
        _ => return Err(invalid()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_nbt::snbt;

    fn tag(text: &str) -> Tag {
        snbt::parse(text).unwrap()
    }

    #[test]
    fn paths_resolve_and_write() {
        let mut root = tag("{a:{b:[1,2,3]},s:\"x\"}");
        let path = NbtPath::parse("a.b[1]").unwrap();
        assert_eq!(path.get(&root), Some(Tag::Int(2)));
        path.set(&mut root, Tag::Int(9)).unwrap();
        assert_eq!(snbt::format(&root), "{a:{b:[1,9,3]},s:\"x\"}");
        assert_eq!(NbtPath::parse("missing.leaf").unwrap().get(&root), None);
        assert!(NbtPath::parse("a..b").is_err());
        assert!(NbtPath::parse("a[").is_err());
    }

    #[test]
    fn merge_remove_and_list_edits() {
        let mut root = tag("{a:{b:1}}");
        let path = NbtPath::parse("a").unwrap();
        assert!(path.merge(&mut root, &tag("{c:2}")).unwrap());
        assert_eq!(snbt::format(&root), "{a:{b:1,c:2}}");
        assert!(!path.merge(&mut root, &tag("{b:1}")).unwrap());
        let list_path = NbtPath::parse("l").unwrap();
        append(&list_path, &mut root, Tag::Int(1)).unwrap();
        append(&list_path, &mut root, Tag::Int(2)).unwrap();
        prepend(&list_path, &mut root, Tag::Int(0)).unwrap();
        assert_eq!(snbt::format(&root), "{a:{b:1,c:2},l:[0,1,2]}");
        assert_eq!(list_path.remove(&mut root).unwrap(), tag("[0,1,2]"));
        assert!(NbtPath::parse("a.b").unwrap().remove(&mut root).is_ok());
        assert!(NbtPath::parse("nope").unwrap().remove(&mut root).is_err());
    }
}
