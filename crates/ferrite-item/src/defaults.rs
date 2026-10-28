//! Default components per item, compiled by `build.rs` from the vanilla 26.3 item report.

use std::collections::BTreeMap;

use ferrite_nbt::Tag;

use crate::component::component_id;

include!(concat!(env!("OUT_DIR"), "/items.rs"));

/// `minecraft:air`, item protocol id 0; a stack of air is empty.
pub const AIR: u16 = 0;

impl ItemDefaults {
    /// The item's complete default component set, keyed by component type id.
    ///
    /// The values are raw NBT; use [`crate::Component::from_nbt`] for typed values. The set is
    /// decoded from the embedded table on each call, so hot paths should use the scalar columns
    /// ([`ItemDefaults::max_stack_size`], [`ItemDefaults::max_damage`], [`ItemDefaults::rarity`])
    /// or [`crate::ItemStack::max_stack_size`] instead.
    pub fn components(&self) -> BTreeMap<u16, Tag> {
        let start = self.offset as usize;
        let end = start + self.len as usize;
        let tag = ferrite_nbt::binary::read_network(&DEFAULTS_NBT[start..end])
            .expect("generated default component NBT is valid");
        let Tag::Compound(components) = tag else {
            panic!("generated default components are not an NBT compound");
        };
        components
            .into_iter()
            .map(|(name, value)| {
                let id = component_id(&name).unwrap_or_else(|| {
                    panic!("generated default component {name:?} is not registered")
                });
                (id, value)
            })
            .collect()
    }
}

/// Number of items in the `minecraft:item` registry.
pub fn item_count() -> usize {
    ITEM_COUNT
}

/// Every item's defaults, sorted by name.
pub fn items() -> impl Iterator<Item = &'static ItemDefaults> {
    ITEMS.iter()
}

/// The defaults of an item protocol id.
pub fn item_defaults(id: u16) -> Option<&'static ItemDefaults> {
    let index = *ITEM_BY_ID.get(usize::from(id))?;
    ITEMS.get(usize::from(index))
}

/// Protocol id of an item name, with or without the `minecraft:` prefix.
pub fn item_id(name: &str) -> Option<u16> {
    let wanted = name.strip_prefix("minecraft:").unwrap_or(name);
    let index = ITEMS
        .binary_search_by(|item| name_without_namespace(item).cmp(wanted))
        .ok()?;
    ITEMS.get(index).map(|item| item.id)
}

/// The defaults of an item name, with or without the `minecraft:` prefix.
pub fn item_defaults_by_name(name: &str) -> Option<&'static ItemDefaults> {
    item_id(name).and_then(item_defaults)
}

/// Name of an item protocol id.
pub fn item_name(id: u16) -> Option<&'static str> {
    item_defaults(id).map(|defaults| defaults.name)
}

/// The complete default component set of an item, keyed by component type id.
pub fn default_components(id: u16) -> Option<BTreeMap<u16, Tag>> {
    item_defaults(id).map(ItemDefaults::components)
}

fn name_without_namespace(item: &ItemDefaults) -> &str {
    item.name.strip_prefix("minecraft:").unwrap_or(item.name)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::component::COMPONENT_COUNT;
    use crate::component_ids;

    #[test]
    fn air_is_item_zero() {
        assert_eq!(item_name(AIR), Some("minecraft:air"));
        assert_eq!(item_id("air"), Some(AIR));
        assert_eq!(item_id("minecraft:air"), Some(AIR));
        assert_eq!(item_count(), 1658);
    }

    #[test]
    fn lookups_accept_both_spellings() {
        let stone = item_id("stone").expect("stone is an item");
        assert_eq!(item_id("minecraft:stone"), Some(stone));
        assert_eq!(item_name(stone), Some("minecraft:stone"));
        assert_eq!(
            item_defaults(stone).map(|defaults| defaults.name),
            Some("minecraft:stone")
        );
        assert_eq!(
            item_defaults_by_name("minecraft:stone").map(|defaults| defaults.id),
            Some(stone)
        );
        assert_eq!(item_id("not_an_item"), None);
        assert_eq!(item_name(u16::MAX), None);
        assert_eq!(item_defaults(u16::MAX), None);
        assert_eq!(item_defaults_by_name("not_an_item"), None);
    }

    #[test]
    fn every_item_has_an_id_and_defaults() {
        assert_eq!(items().count(), item_count());
        let mut ids = BTreeSet::new();
        let mut previous: Option<&str> = None;
        for item in items() {
            assert!(item.name.starts_with("minecraft:"), "{}", item.name);
            if let Some(previous) = previous {
                assert!(previous < item.name, "items are sorted by name");
            }
            previous = Some(item.name);
            assert!(ids.insert(item.id), "{} repeats an id", item.name);
            assert_eq!(item_id(item.name), Some(item.id));
            assert_eq!(
                item_defaults(item.id).map(|defaults| defaults.name),
                Some(item.name)
            );

            let components = item.components();
            assert!(
                !components.is_empty(),
                "{} has no default components",
                item.name
            );
            assert!(
                components
                    .keys()
                    .all(|id| usize::from(*id) < COMPONENT_COUNT)
            );
            assert!(
                components.contains_key(&component_ids::MAX_STACK_SIZE),
                "{} has no default max_stack_size",
                item.name
            );
        }
        assert_eq!(ids.len(), item_count());
        assert_eq!(ids.first(), Some(&AIR));
        assert_eq!(
            ids.last().map(|id| usize::from(*id)),
            Some(item_count() - 1)
        );
    }

    #[test]
    fn default_components_are_typed_where_expected() {
        let sword =
            item_defaults(item_id("diamond_sword").expect("diamond_sword")).expect("defaults");
        assert_eq!(sword.max_stack_size, 1);
        assert_eq!(sword.max_damage, Some(1561));
        assert_eq!(sword.rarity, crate::Rarity::Common);

        let stone = item_defaults(item_id("stone").expect("stone")).expect("defaults");
        assert_eq!(stone.max_stack_size, 64);
        assert_eq!(stone.max_damage, None);

        let components =
            default_components(item_id("diamond_sword").expect("diamond_sword")).expect("defaults");
        assert_eq!(
            components.get(&component_ids::MAX_DAMAGE),
            Some(&Tag::Int(1561))
        );
        assert_eq!(
            components.get(&component_ids::ITEM_NAME).cloned(),
            Some(Tag::Compound(
                [(
                    "translate".to_owned(),
                    Tag::String("item.minecraft.diamond_sword".to_owned())
                )]
                .into_iter()
                .collect()
            ))
        );
    }
}
