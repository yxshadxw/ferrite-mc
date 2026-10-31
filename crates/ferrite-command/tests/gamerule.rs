use ferrite_command::gamerule::{GameRules, RuleKind, RuleValue};
use ferrite_nbt::{Compound, Tag};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn reports() -> Option<PathBuf> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3/generated/reports");
    path.is_dir().then_some(path)
}

#[test]
fn matches_the_vanilla_command_tree() {
    let Some(reports) = reports() else {
        return;
    };
    let tree: serde_json::Value =
        serde_json::from_slice(&std::fs::read(reports.join("commands.json")).unwrap()).unwrap();
    let children = tree["children"]["gamerule"]["children"]
        .as_object()
        .unwrap();
    let mut expected = BTreeMap::new();
    for (label, node) in children {
        let Some(name) = label.strip_prefix("minecraft:") else {
            continue;
        };
        // Each namespaced literal must have the same parser and bounds as the plain one.
        assert_eq!(
            node["children"]["value"]["parser"], children[name]["children"]["value"]["parser"],
            "{label}"
        );
        let value = &children[name]["children"]["value"];
        let properties = &value["properties"];
        let kind = match value["parser"].as_str().unwrap() {
            "brigadier:bool" => RuleKind::Boolean,
            "brigadier:integer" => RuleKind::Integer {
                min: properties["min"].as_i64().unwrap_or(i32::MIN as i64) as i32,
                max: properties["max"].as_i64().unwrap_or(i32::MAX as i64) as i32,
            },
            other => panic!("unexpected parser for {name}: {other}"),
        };
        assert!(
            expected.insert(name.to_owned(), kind).is_none(),
            "duplicate rule {name}"
        );
    }
    let actual: BTreeMap<String, RuleKind> = GameRules::specs()
        .iter()
        .map(|spec| (spec.name.to_owned(), spec.kind))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn matches_the_vanilla_game_rule_registry() {
    let Some(reports) = reports() else {
        return;
    };
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(reports.join("registries.json")).unwrap()).unwrap();
    let entries = registry["minecraft:game_rule"]["entries"]
        .as_object()
        .unwrap();
    let expected: BTreeSet<String> = entries
        .keys()
        .map(|key| key.strip_prefix("minecraft:").unwrap().to_owned())
        .collect();
    let actual: BTreeSet<String> = GameRules::specs()
        .iter()
        .map(|spec| spec.name.to_owned())
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 59);
}

#[test]
fn validates_types_bounds_aliases_and_nbt_roundtrip() {
    let mut rules = GameRules::default();
    assert_eq!(rules.get("random_tick_speed"), Some(RuleValue::Integer(3)));
    assert_eq!(rules.get_bool("keep_inventory"), Some(false));

    // Boolean rules only accept true/false; integer rules enforce the report bounds.
    assert!(rules.set("keep_inventory", "1").is_err());
    assert!(rules.set("random_tick_speed", "-1").is_err());
    assert!(rules.set("max_snow_accumulation_height", "9").is_err());
    assert!(rules.set("no_such_rule", "true").is_err());

    rules.set("random_tick_speed", "0").unwrap();
    rules.set("advance_time", "false").unwrap();
    rules.set("max_snow_accumulation_height", "8").unwrap();
    assert_eq!(
        rules.get("minecraft:random_tick_speed"),
        Some(RuleValue::Integer(0))
    );
    assert_eq!(rules.get("randomTickSpeed"), Some(RuleValue::Integer(0)));
    // The pre-26.3 names map onto the rules they were renamed to.
    assert_eq!(
        rules.get("doDaylightCycle"),
        Some(RuleValue::Boolean(false))
    );
    assert_eq!(rules.get("doMobSpawning"), Some(RuleValue::Boolean(true)));

    let tag = rules.to_nbt(5023);
    let Tag::Compound(root) = &tag else {
        panic!("rules encode as a compound");
    };
    assert!(root.contains_key("data"));
    assert_eq!(root.get("DataVersion"), Some(&Tag::Int(5023)));
    let restored = GameRules::from_nbt(&tag).unwrap();
    assert_eq!(
        restored.get("random_tick_speed"),
        Some(RuleValue::Integer(0))
    );
    assert_eq!(
        restored.get("advance_time"),
        Some(RuleValue::Boolean(false))
    );
    assert_eq!(
        restored.get("max_snow_accumulation_height"),
        Some(RuleValue::Integer(8))
    );
}

#[test]
fn reads_the_pre_26_3_level_dat_compound() {
    let mut stored = Compound::new();
    stored.insert("randomTickSpeed".into(), Tag::String("0".into()));
    stored.insert("keepInventory".into(), Tag::String("true".into()));
    stored.insert("notARule".into(), Tag::String("1".into()));
    let mut data = Compound::new();
    data.insert("GameRules".into(), Tag::Compound(stored));
    let mut root = Compound::new();
    root.insert("Data".into(), Tag::Compound(data));

    let rules = GameRules::from_level_dat(&Tag::Compound(root)).unwrap();
    assert_eq!(rules.get_integer("random_tick_speed"), Some(0));
    assert_eq!(rules.get_bool("keep_inventory"), Some(true));
    assert_eq!(rules.get("notARule"), None);
}
