//! Tests for the vanilla `server.properties` model.

use ferrite_config::{
    Difficulty, FerriteConfig, Gamemode, PropertiesDocument, RegionFileCompression,
    ServerProperties, ServerPropertiesDocument,
};

const FIXTURE: &str = include_str!("fixtures/server.properties");

fn parse_fixture() -> (PropertiesDocument, ServerProperties) {
    let document = PropertiesDocument::parse(FIXTURE);
    let typed = ServerProperties::from_document(&document).expect("fixture should parse");
    (document, typed)
}

#[test]
fn real_vanilla_server_properties_loads() {
    let (document, typed) = parse_fixture();

    assert_eq!(
        document.len(),
        ServerProperties::KEY_TABLE.len(),
        "the 26.3 fixture should contain exactly the keys the schema models"
    );
    assert_eq!(document.len(), 69);

    assert_eq!(typed.server_port, 25_565);
    assert_eq!(typed.motd, "A Minecraft Server");
    assert_eq!(typed.level_name, "world");
    assert_eq!(typed.level_type, "minecraft:normal");
    assert_eq!(typed.difficulty, Difficulty::Easy);
    assert_eq!(typed.gamemode, Gamemode::Survival);
    assert_eq!(
        typed.region_file_compression,
        RegionFileCompression::Deflate
    );
    assert!(typed.white_list);
    assert_eq!(typed.max_tick_time, 60_000);
    assert_eq!(typed.simulation_distance, 10);

    // Every compiled-in default must match the real vanilla file. The fixture blanks the
    // randomly generated management-server-secret, which vanilla never has a fixed default
    // for, so that one key is skipped.
    for key in ServerProperties::KEY_TABLE {
        if key.key == "management-server-secret" {
            continue;
        }
        assert_eq!(
            ServerProperties::default_key_value(key.key).as_deref(),
            document.get(key.key),
            "compiled-in default differs from the vanilla 26.3 file for `{}`",
            key.key
        );
    }

    // Every key in the real file must be modeled, and every value must agree with the
    // typed model (this catches both missing keys and wrong defaults).
    for (key, value) in document.iter() {
        assert!(
            ServerProperties::default_key_value(key).is_some(),
            "fixture key `{key}` is not modeled"
        );
        assert_eq!(
            typed.get_key(key).as_deref(),
            Some(value),
            "typed value differs for `{key}`"
        );
    }
}

#[test]
fn defaults_are_complete_and_round_trip() {
    let defaults = ServerPropertiesDocument::default();
    assert_eq!(defaults.document().len(), ServerProperties::KEY_TABLE.len());

    let rendered = defaults.to_string_with_comments();
    assert!(rendered.starts_with("#Minecraft server properties\n"));
    assert!(
        rendered.contains("# Port the server listens on.\nserver-port=25565\n"),
        "generated defaults should carry per-key comments:\n{rendered}"
    );

    let reparsed = PropertiesDocument::parse(&rendered);
    assert_eq!(reparsed.len(), ServerProperties::KEY_TABLE.len());
    for key in ServerProperties::KEY_TABLE {
        assert!(
            reparsed.get(key.key).is_some(),
            "generated file is missing `{}`",
            key.key
        );
    }

    let typed = ServerProperties::from_document(&reparsed).expect("generated file should parse");
    assert_eq!(typed, ServerProperties::default());
}

#[test]
fn unknown_keys_survive_rewrite() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut text = FIXTURE.to_string();
    text.push_str("custom-key=hello\nanother.unknown=42\n");
    std::fs::write(dir.path().join("server.properties"), text).expect("write fixture");

    let config = FerriteConfig::load(dir.path()).expect("load config");
    assert_eq!(
        config.server_properties_document().get("custom-key"),
        Some("hello")
    );

    config.save().expect("save config");
    let path = dir.path().join("server.properties");
    let written = std::fs::read_to_string(&path).expect("read saved file");
    assert!(written.contains("custom-key=hello"));
    assert!(written.contains("another.unknown=42"));
    assert!(written.contains("#Minecraft server properties"));

    let reloaded = ServerPropertiesDocument::load(&path)
        .expect("reload")
        .expect("file exists");
    assert_eq!(reloaded.document().get("custom-key"), Some("hello"));
    assert_eq!(reloaded.document().get("another.unknown"), Some("42"));
    assert_eq!(reloaded.typed().server_port, 25_565);

    // Saving again must not duplicate or drop anything.
    config.save().expect("second save");
    let written_again = std::fs::read_to_string(&path).expect("read saved file again");
    assert_eq!(written, written_again);
}

#[test]
fn properties_escaping_round_trips() {
    let text =
        "level-type=minecraft\\:normal\nweird=line\\nbreak\nunicode=\\u00e9\nspace=hello\\ world\n";
    let document = PropertiesDocument::parse(text);
    assert_eq!(document.get("level-type"), Some("minecraft:normal"));
    assert_eq!(document.get("weird"), Some("line\nbreak"));
    assert_eq!(document.get("unicode"), Some("é"));
    assert_eq!(document.get("space"), Some("hello world"));

    let rendered = document.to_plain_string();
    let reparsed = PropertiesDocument::parse(&rendered);
    assert_eq!(reparsed.len(), document.len());
    for (key, value) in document.iter() {
        assert_eq!(
            reparsed.get(key),
            Some(value),
            "round-trip failed for `{key}`"
        );
    }
    assert!(rendered.contains("level-type=minecraft\\:normal"));
}
