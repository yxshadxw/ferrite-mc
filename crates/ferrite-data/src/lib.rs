//! Vanilla data (registries, block states, packet ids) extracted from the official server jar.
//!
//! The tables are generated at build time by `build.rs` from the data generator output in
//! `vanilla/26.3/generated/`. When that directory is missing, the build fails with a pointer to
//! `python tools/fetch_vanilla.py`, which produces it.
//!
//! All lookups are binary searches over tables sorted at generation time; nothing is built or
//! allocated at runtime, and names are accepted with or without the `minecraft:` namespace.

/// Strips an optional `minecraft:` namespace so lookups accept both spellings of a name.
fn key(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

pub mod blocks {
    //! Global block state ids (0..[`STATE_COUNT`]) and their properties.

    /// One block property: its name, its values (sorted) and the value the default state uses.
    struct Prop {
        name: &'static str,
        values: &'static [&'static str],
        /// Index into `values`, applied when a lookup omits the property.
        default: u16,
    }

    /// A block, with the state ids of the whole block indexed by property combination.
    struct Block {
        name: &'static str,
        props: &'static [Prop],
        /// Global state id per mixed-radix index of the `props` values (first property is the
        /// most significant digit, values in their order in [`Prop::values`]).
        map: &'static [u16],
    }

    include!(concat!(env!("OUT_DIR"), "/blocks.rs"));

    /// The block with the given name, if vanilla has one.
    fn locate(name: &str) -> Option<&'static Block> {
        let name = super::key(name);
        let index = BLOCKS
            .binary_search_by(|block| super::key(block.name).cmp(name))
            .ok()?;
        BLOCKS.get(index)
    }

    /// Global block state id for a block name (with or without `"minecraft:"`) and property
    /// values. Missing properties use the block's default.
    ///
    /// Returns `None` for an unknown block, an unknown or misspelled property, or a value that
    /// the property does not take.
    pub fn state_id(name: &str, props: &[(&str, &str)]) -> Option<u16> {
        let block = locate(name)?;
        // Unknown property names are an error rather than being silently dropped.
        for (name, _) in props {
            if !block.props.iter().any(|prop| prop.name == *name) {
                return None;
            }
        }

        let mut index = 0;
        let mut stride = block.map.len();
        for prop in block.props {
            stride /= prop.values.len();
            let position = match props.iter().find(|(name, _)| *name == prop.name) {
                Some((_, value)) => prop.values.binary_search(value).ok()?,
                None => prop.default as usize,
            };
            index += position * stride;
        }
        block.map.get(index).copied()
    }

    /// Global block state id of the default state of a block.
    pub fn default_state(name: &str) -> Option<u16> {
        state_id(name, &[])
    }

    /// Name and property list of a state id.
    pub fn state(id: u16) -> Option<(&'static str, &'static [(&'static str, &'static str)])> {
        let index = id as usize;
        let block = BLOCKS.get(*STATE_BLOCK.get(index)? as usize)?;
        let props = PROP_SETS.get(*STATE_PROP_SET.get(index)? as usize)?;
        Some((block.name, props))
    }
}

pub mod registries {
    //! Built-in registries (`reports/registries.json`) and data-driven registry entry names
    //! (`data/minecraft/**`).

    /// One built-in registry: entry names (sorted) with their protocol ids.
    struct Registry {
        name: &'static str,
        names: &'static [&'static str],
        ids: &'static [i32],
    }

    include!(concat!(env!("OUT_DIR"), "/registries.rs"));

    /// Built-in registries from `reports/registries.json`, e.g.
    /// `("minecraft:item", "minecraft:stone") -> protocol id`.
    pub fn id(registry: &str, entry: &str) -> Option<i32> {
        let registry = super::key(registry);
        let entry = super::key(entry);
        let index = REGISTRIES
            .binary_search_by(|candidate| super::key(candidate.name).cmp(registry))
            .ok()?;
        let registry = REGISTRIES.get(index)?;
        let index = registry
            .names
            .binary_search_by(|name| super::key(name).cmp(entry))
            .ok()?;
        registry.ids.get(index).copied()
    }

    /// Entry names of a built-in registry, sorted by name.
    pub fn entries(registry: &str) -> Option<&'static [&'static str]> {
        let registry = super::key(registry);
        let index = REGISTRIES
            .binary_search_by(|candidate| super::key(candidate.name).cmp(registry))
            .ok()?;
        REGISTRIES.get(index).map(|registry| registry.names)
    }

    /// Entry names of a data-driven registry: the entry names of
    /// `data/minecraft/<registry>/**/*.json`, sorted the way vanilla sorts them when sending a
    /// registry (by name).
    ///
    /// `registry` is the directory below `data/minecraft`, e.g. `"worldgen/biome"`,
    /// `"dimension_type"` or `"damage_type"`; `minecraft/` is accepted as a prefix.
    pub fn data_entries(registry: &str) -> Option<&'static [&'static str]> {
        let registry = super::key(registry);
        let index = DATA
            .binary_search_by(|(name, _)| name.cmp(&registry))
            .ok()?;
        DATA.get(index).map(|(_, entries)| *entries)
    }
}

pub mod packets {
    //! Packet ids per connection state and direction (`reports/packets.json`).

    /// One `(state, bound)` table: packet names (sorted) with their protocol ids.
    struct Table {
        state: &'static str,
        bound: &'static str,
        names: &'static [&'static str],
        ids: &'static [i32],
    }

    include!(concat!(env!("OUT_DIR"), "/packets.rs"));

    /// Packet id from `reports/packets.json`: `(state, bound, "minecraft:name") -> id`, with
    /// `state` in `{"handshake", "status", "login", "configuration", "play"}` and `bound` in
    /// `{"clientbound", "serverbound"}`. The `minecraft:` prefix is optional.
    pub fn id(state: &str, bound: &str, name: &str) -> Option<i32> {
        let name = super::key(name);
        let table = TABLES
            .iter()
            .find(|table| table.state == state && table.bound == bound)?;
        let index = table
            .names
            .binary_search_by(|candidate| super::key(candidate).cmp(name))
            .ok()?;
        table.ids.get(index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::{blocks, packets, registries};

    #[test]
    fn stone_state_matches_its_default() {
        assert_eq!(
            blocks::state_id("stone", &[]),
            blocks::default_state("minecraft:stone")
        );
        assert!(blocks::default_state("stone").is_some());
        assert_eq!(
            blocks::default_state("minecraft:stone"),
            Some(1),
            "stone is not state 1 in this report"
        );
    }

    #[test]
    fn air_is_state_zero() {
        assert_eq!(blocks::state_id("air", &[]), Some(0));
        assert_eq!(blocks::default_state("minecraft:air"), Some(0));
    }

    #[test]
    fn multi_property_state_round_trips() {
        let expected = [
            ("facing", "east"),
            ("half", "top"),
            ("shape", "straight"),
            ("waterlogged", "false"),
        ];
        let id = blocks::state_id("oak_stairs", &expected).expect("oak_stairs state exists");
        let (name, props) = blocks::state(id).expect("state id is in range");
        assert_eq!(name, "minecraft:oak_stairs");
        assert_eq!(props.len(), expected.len(), "unexpected props {props:?}");
        for (key, value) in expected {
            assert!(
                props.contains(&(key, value)),
                "{key}={value} missing from {props:?}"
            );
        }

        // Property order does not matter, and omitted properties fall back to the default.
        let reordered = blocks::state_id(
            "minecraft:oak_stairs",
            &[
                ("waterlogged", "false"),
                ("shape", "straight"),
                ("half", "top"),
                ("facing", "east"),
            ],
        );
        assert_eq!(reordered, Some(id));

        let partial = blocks::state_id("oak_stairs", &[("facing", "east")]).expect("state exists");
        let (_, props) = blocks::state(partial).expect("state id is in range");
        for defaulted in [
            ("half", "bottom"),
            ("shape", "straight"),
            ("waterlogged", "false"),
        ] {
            assert!(
                props.contains(&defaulted),
                "{defaulted:?} missing from {props:?}"
            );
        }
    }

    #[test]
    fn unknown_lookups_are_none() {
        assert_eq!(blocks::state_id("not_a_block", &[]), None);
        assert_eq!(blocks::state_id("stone", &[("facing", "north")]), None);
        assert_eq!(blocks::state_id("stone", &[("facing", "sideways")]), None);
        assert_eq!(blocks::state_id("oak_stairs", &[("facing", "up")]), None);
        assert_eq!(blocks::default_state("oak_stairs_typo"), None);
        assert_eq!(blocks::state(u16::MAX), None);
    }

    #[test]
    fn every_state_round_trips() {
        assert_eq!(blocks::STATE_COUNT, 35_723, "unexpected state count");
        for id in 0..blocks::STATE_COUNT as u16 {
            let (name, props) =
                blocks::state(id).unwrap_or_else(|| panic!("state {id} is missing"));
            assert_eq!(
                blocks::state_id(name, props),
                Some(id),
                "{name} {props:?} does not round-trip"
            );
        }
    }

    #[test]
    fn built_in_registries() {
        assert_eq!(registries::id("minecraft:item", "minecraft:air"), Some(0));
        assert_eq!(registries::id("item", "air"), Some(0));
        assert_eq!(
            registries::id("minecraft:block", "minecraft:stone"),
            Some(1)
        );
        assert_eq!(
            registries::id("minecraft:item", "minecraft:not_an_item"),
            None
        );
        assert_eq!(registries::id("minecraft:not_a_registry", "air"), None);

        let items = registries::entries("minecraft:item").expect("item registry exists");
        assert!(items.contains(&"minecraft:air"));
        assert!(items.windows(2).all(|pair| pair[0] < pair[1]), "sorted");
        assert_eq!(registries::entries("minecraft:not_a_registry"), None);
    }

    #[test]
    fn data_driven_registries() {
        let biomes = registries::data_entries("worldgen/biome").expect("biomes are generated");
        assert!(biomes.contains(&"minecraft:plains"), "{biomes:?}");
        assert!(biomes.windows(2).all(|pair| pair[0] < pair[1]), "sorted");

        let dimensions = registries::data_entries("dimension_type").expect("dimension types");
        assert!(dimensions.contains(&"minecraft:overworld"));
        assert!(registries::data_entries("damage_type").is_some());
        assert_eq!(registries::data_entries("not_a_registry"), None);
    }

    #[test]
    fn packet_ids() {
        assert_eq!(
            packets::id("handshake", "serverbound", "minecraft:intention"),
            Some(0)
        );
        assert_eq!(
            packets::id("status", "clientbound", "minecraft:status_response"),
            Some(0)
        );
        assert_eq!(
            packets::id("status", "serverbound", "minecraft:status_request"),
            Some(0)
        );
        assert_eq!(packets::id("status", "serverbound", "intention"), None);
        assert_eq!(
            packets::id("handshake", "clientbound", "minecraft:intention"),
            None
        );
        assert_eq!(packets::id("play", "serverbound", "not_a_packet"), None);
    }
}
