//! `update_tags` (configuration and play): tag entries are network IDs of their registry,
//! so they are remapped with the static tables or the connection's synchronized-registry
//! remaps. Entries without a counterpart are dropped (never replaced by a default, which
//! would add e.g. air to `#minecraft:mineable/pickaxe`); registries the older client does
//! not know are dropped entirely.

use ferrite_protocol::packets::configuration::{RegistryTags, Tag, UpdateTags};
use ferrite_protocol::{Decode, Encode, VarInt};

use crate::TranslateError;
use crate::registry_sync::DynamicRegistries;
use crate::tables::PairTables;

pub fn rewrite_update_tags(
    body: &[u8],
    tables: &PairTables,
    registries: &DynamicRegistries,
) -> Result<Vec<u8>, TranslateError> {
    let mut buf = body;
    let input = UpdateTags::decode(&mut buf)?;
    let mut output = Vec::with_capacity(input.registries.len());
    for RegistryTags { registry, tags } in input.registries {
        let map: Box<dyn Fn(i32) -> Option<i32>> = if let Some(table) = tables.registry(&registry) {
            Box::new(move |id| table.down(id))
        } else if let Some(remap) = registries.get(&registry) {
            if registries.is_dropped(&registry) {
                continue;
            }
            let remap = remap.clone();
            Box::new(move |id| remap.down(id))
        } else {
            // A registry the older client has neither as a static nor as a synchronized
            // registry; its tags cannot be bound.
            continue;
        };
        let tags = tags
            .into_iter()
            .map(|Tag { name, entries }| {
                let mut seen = std::collections::HashSet::new();
                let entries = entries
                    .into_iter()
                    .filter_map(|VarInt(id)| map(id))
                    .filter(|id| seen.insert(*id))
                    .map(VarInt)
                    .collect();
                Tag { name, entries }
            })
            .collect();
        output.push(RegistryTags { registry, tags });
    }
    let mut out = Vec::with_capacity(body.len());
    UpdateTags { registries: output }.encode(&mut out);
    out.extend_from_slice(buf);
    Ok(out)
}
