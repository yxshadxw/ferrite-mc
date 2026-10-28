//! Tests for the built-in registry tags generated from `data/minecraft/tags/**`.

use ferrite_data::tags;

/// Formats an integer with thousands separators (`20199` -> `20,199`).
fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

#[test]
fn built_in_tag_totals() {
    let registries = tags::registries().len();
    let count = tags::all().len();
    println!(
        "{registries} registries, {count} tags, {} resolved entries",
        grouped(tags::RESOLVED_ENTRIES)
    );

    assert_eq!(tags::REGISTRY_COUNT, registries);
    assert_eq!(tags::TAG_COUNT, count);
    assert_eq!(registries, 20);
    assert_eq!(count, 883);
    assert_eq!(tags::RESOLVED_ENTRIES, 20_199);
}

#[test]
fn logs_contain_oak_log() {
    assert!(tags::contains(
        "minecraft:block",
        "minecraft:logs",
        "minecraft:oak_log"
    ));
    assert!(tags::contains("block", "logs", "oak_log"));
    assert!(tags::contains_name("block", "logs", "oak_log"));
    assert_eq!(
        tags::get("minecraft:block", "minecraft:logs").map(tags::Tag::registry),
        Some("minecraft:block")
    );
    assert!(!tags::contains("block", "logs", "stone"));
}

#[test]
fn pickaxe_contains_stone() {
    let tag = tags::get("minecraft:block", "minecraft:mineable/pickaxe")
        .expect("#minecraft:mineable/pickaxe exists");
    assert!(tag.contains("stone"));
    assert!(tag.contains_name("stone"));
    assert!(tag.contains("minecraft:deepslate"));
    assert!(!tag.contains("oak_planks"));
}

#[test]
fn item_planks_contains_oak_planks() {
    let tag = tags::get("item", "planks").expect("#minecraft:planks exists");
    assert_eq!(tag.name(), "minecraft:planks");
    assert_eq!(tag.registry(), "minecraft:item");
    assert!(tag.contains_name("oak_planks"));
    assert!(tag.contains("minecraft:oak_planks"));
    assert!(!tag.contains("stone"));
}

#[test]
fn unknown_tags_and_registries_are_none() {
    assert!(tags::get("block", "not_a_tag").is_none());
    assert!(tags::get("not_a_registry", "logs").is_none());
    assert!(tags::get("entity_type", "logs").is_none());
    assert!(!tags::contains("block", "not_a_tag", "stone"));
    assert!(!tags::contains_name("item", "planks", "not_an_item"));
}

#[test]
fn tables_are_sorted_and_namespaced() {
    assert!(tags::registries().windows(2).all(|pair| pair[0] < pair[1]));

    let all = tags::all();
    assert!(all.windows(2).all(|pair| {
        (pair[0].registry(), pair[0].name()) < (pair[1].registry(), pair[1].name())
    }));
    for tag in all {
        assert!(tag.registry().starts_with("minecraft:"));
        assert!(tag.name().starts_with("minecraft:"));
        assert!(
            tag.entries().windows(2).all(|pair| pair[0] < pair[1]),
            "{} entries are not sorted and unique",
            tag.name()
        );
        assert!(
            tag.entries()
                .iter()
                .all(|entry| entry.starts_with("minecraft:")),
            "{} has a non-namespaced entry",
            tag.name()
        );
    }
}

#[test]
fn every_registry_has_a_tag() {
    for registry in tags::registries() {
        assert!(
            tags::all().iter().any(|tag| tag.registry() == *registry),
            "{registry} has no tags"
        );
    }
}
