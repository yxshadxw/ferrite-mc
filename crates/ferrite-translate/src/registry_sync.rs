//! Synchronized (data-driven) registries: filtering `registry_data` for an older client and
//! remapping the network IDs that refer to its entries.
//!
//! Network IDs of a synchronized registry are the entry indices of its `registry_data`
//! packet. When the older client cannot resolve an entry (it has no vanilla data for it
//! and the server sends none), the entry is dropped, every later index shifts down, and
//! every ID referring to the registry (chunk biomes, dimension type in login/respawn, tags)
//! has to be remapped with the [`DynamicRemap`] recorded here.

use std::collections::HashMap;

use ferrite_protocol::{Decode, Encode, VarInt};

use crate::TranslateError;
use crate::tables::{PairTables, apply_fallback};

/// Index remap of one synchronized registry, as sent to the older client.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DynamicRemap {
    /// New entry index -> old entry index; `None` if the entry was dropped and no fallback
    /// entry exists.
    pub down: Vec<Option<i32>>,
    /// Entry names in the order the older client received them.
    pub old_entries: Vec<String>,
    /// Old entry index -> new entry index.
    pub up: Vec<i32>,
}

impl DynamicRemap {
    pub fn down(&self, id: i32) -> Option<i32> {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.down.get(i))
            .copied()
            .flatten()
    }

    /// Like [`Self::down`], falling back to entry 0 (the client accepts any valid index).
    pub fn down_or_first(&self, id: i32) -> i32 {
        self.down(id).unwrap_or(0)
    }

    pub fn up(&self, id: i32) -> Option<i32> {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.up.get(i))
            .copied()
    }

    pub fn old_len(&self) -> usize {
        self.old_entries.len()
    }
}

/// The remaps of every synchronized registry sent so far on one connection.
#[derive(Debug, Clone, Default)]
pub struct DynamicRegistries {
    remaps: HashMap<String, DynamicRemap>,
    /// Registries whose `registry_data` was dropped entirely (unknown to the older client).
    dropped: Vec<String>,
}

impl DynamicRegistries {
    pub fn get(&self, registry: &str) -> Option<&DynamicRemap> {
        self.remaps.get(registry)
    }

    pub fn is_dropped(&self, registry: &str) -> bool {
        self.dropped.iter().any(|r| r == registry)
    }

    pub fn insert(&mut self, registry: String, remap: DynamicRemap) {
        self.remaps.insert(registry, remap);
    }
}

struct Entry<'a> {
    name: String,
    /// Raw network NBT of the entry, if the server sends its data inline.
    data: Option<&'a [u8]>,
}

/// Rewrite one `registry_data` body (`Identifier registry | Prefixed Array of (Identifier,
/// Prefixed Optional NBT)`). Returns `None` when the whole packet must be dropped.
pub fn rewrite_registry_data(
    body: &[u8],
    tables: &PairTables,
    registries: &mut DynamicRegistries,
) -> Result<Option<Vec<u8>>, TranslateError> {
    let mut buf = body;
    let registry = String::decode(&mut buf)?;
    let n = usize::try_from(VarInt::decode(&mut buf)?.0)
        .map_err(|_| TranslateError::Invalid("registry entry count"))?;
    let mut entries = Vec::with_capacity(n.min(buf.len()));
    for _ in 0..n {
        let name = String::decode(&mut buf)?;
        let data = if bool::decode(&mut buf)? {
            let start = buf;
            crate::nbt::skip(&mut buf)?;
            Some(&start[..start.len() - buf.len()])
        } else {
            None
        };
        entries.push(Entry { name, data });
    }
    if !buf.is_empty() {
        return Err(TranslateError::Invalid("trailing bytes in registry_data"));
    }

    let Some(known) = tables.old_data_entries(&registry) else {
        // The older client does not synchronize this registry at all.
        registries.dropped.push(registry.clone());
        registries.insert(
            registry,
            DynamicRemap {
                down: vec![None; entries.len()],
                ..DynamicRemap::default()
            },
        );
        return Ok(None);
    };

    let mut remap = DynamicRemap::default();
    // Entries with inline data are always resolvable; vanilla ones only if the older
    // version ships them.
    let kept: Vec<bool> = entries
        .iter()
        .map(|e| e.data.is_some() || known.binary_search(&e.name.as_str()).is_ok())
        .collect();
    for (index, entry) in entries.iter().enumerate() {
        if kept[index] {
            remap.down.push(Some(remap.old_entries.len() as i32));
            remap.old_entries.push(entry.name.clone());
            remap.up.push(index as i32);
        } else {
            remap.down.push(None);
        }
    }
    // Dropped entries resolve through the pair's fallback rules when the target was kept.
    for (index, entry) in entries.iter().enumerate() {
        if remap.down[index].is_none()
            && let Some(target) = apply_fallback(tables.fallbacks, &registry, &entry.name)
        {
            remap.down[index] = remap
                .old_entries
                .iter()
                .position(|e| *e == target)
                .map(|i| i as i32);
        }
    }

    let mut out = Vec::with_capacity(body.len());
    registry.encode(&mut out);
    VarInt(remap.old_entries.len() as i32).encode(&mut out);
    for (entry, _) in entries.iter().zip(&kept).filter(|(_, kept)| **kept) {
        entry.name.encode(&mut out);
        match entry.data {
            Some(nbt) => {
                true.encode(&mut out);
                out.extend_from_slice(nbt);
            }
            None => false.encode(&mut out),
        }
    }
    registries.insert(registry, remap);
    Ok(Some(out))
}
