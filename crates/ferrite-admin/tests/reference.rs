//! Byte-for-byte compatibility with files written by the real 26.3 server.
//!
//! The constants below are the contents of `ops.json`, `whitelist.json`,
//! `banned-players.json`, `banned-ips.json` and `usercache.json` after running
//! `/op TestUser`, `/whitelist add TestUser`, `/ban TestUser griefing`,
//! `/ban-ip 203.0.113.9 spam` and `stop`. Note the absence of trailing newlines
//! and the compact `usercache.json`.

mod common;

use std::fs;
use std::path::Path;

use common::TestClock;
use ferrite_admin::{AdminLists, Timestamp};
use uuid::Uuid;

const TEST_USER: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
const SECOND_USER: &str = "8667ba71-b85a-4004-af54-457a9734eed7";

const OPS_REF: &str = r#"[
  {
    "uuid": "069a79f4-44e9-4726-a5be-fca90e38aaf5",
    "name": "TestUser",
    "level": 4,
    "bypassesPlayerLimit": false
  }
]"#;

const WHITELIST_REF: &str = r#"[
  {
    "uuid": "069a79f4-44e9-4726-a5be-fca90e38aaf5",
    "name": "TestUser"
  }
]"#;

const BANNED_PLAYERS_REF: &str = r#"[
  {
    "uuid": "069a79f4-44e9-4726-a5be-fca90e38aaf5",
    "name": "TestUser",
    "created": "2026-10-28 23:33:38 +0900",
    "source": "Server",
    "expires": "forever",
    "reason": "griefing"
  }
]"#;

const BANNED_IPS_REF: &str = r#"[
  {
    "ip": "203.0.113.9",
    "created": "2026-10-28 23:33:40 +0900",
    "source": "Server",
    "expires": "forever",
    "reason": "spam"
  }
]"#;

const USERCACHE_REF: &str = r#"[{"uuid":"069a79f4-44e9-4726-a5be-fca90e38aaf5","name":"TestUser","expiresOn":"2027-06-01 00:00:00 +0900"}]"#;

const USERCACHE_TWO_USERS_REF: &str = r#"[{"uuid":"069a79f4-44e9-4726-a5be-fca90e38aaf5","name":"TestUser","expiresOn":"2027-06-01 00:00:00 +0900"},{"uuid":"8667ba71-b85a-4004-af54-457a9734eed7","name":"SecondUser","expiresOn":"2027-06-01 00:00:00 +0900"}]"#;

const FIXTURES: [(&str, &str); 5] = [
    ("ops.json", OPS_REF),
    ("whitelist.json", WHITELIST_REF),
    ("banned-players.json", BANNED_PLAYERS_REF),
    ("banned-ips.json", BANNED_IPS_REF),
    ("usercache.json", USERCACHE_REF),
];

fn read(dir: &Path, file: &str) -> String {
    fs::read_to_string(dir.join(file)).unwrap_or_else(|error| panic!("reading {file}: {error}"))
}

fn write_fixtures(dir: &Path) {
    for (file, contents) in FIXTURES {
        fs::write(dir.join(file), contents)
            .unwrap_or_else(|error| panic!("writing {file}: {error}"));
    }
}

#[test]
fn new_lists_serialize_exactly_like_vanilla() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let clock = TestClock::new("2026-10-28 23:33:38 +0900");
    let lists = AdminLists::with_clock(dir.path(), clock.clone())?;
    let uuid = Uuid::parse_str(TEST_USER)?;

    lists.ops.add_op(uuid, "TestUser", 4, false);
    lists.whitelist.add(uuid, "TestUser");
    lists.bans.ban(uuid, "TestUser", "griefing", "Server", None);

    clock.set("2026-10-28 23:33:40 +0900");
    lists.bans.ban_ip("203.0.113.9", "spam", "Server", None);

    // Eight weeks before the usercache expiry vanilla wrote.
    clock.set("2027-04-06 00:00:00 +0900");
    lists.usercache.cache("TestUser", uuid);
    lists.flush()?;

    for (file, expected) in FIXTURES {
        assert_eq!(
            read(dir.path(), file),
            expected,
            "{file} is not vanilla-compatible"
        );
    }
    Ok(())
}

#[test]
fn loaded_vanilla_files_parse_and_stay_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    write_fixtures(dir.path());
    let clock = TestClock::new("2026-10-28 12:00:00 +0900");
    let lists = AdminLists::with_clock(dir.path(), clock)?;
    let uuid = Uuid::parse_str(TEST_USER)?;

    assert_eq!(lists.ops.level_of(uuid), Some(4));
    assert!(lists.whitelist.contains(uuid));
    assert!(lists.whitelist.contains_name("testuser"));

    let ban = lists.bans.player_ban(uuid).expect("player ban");
    assert_eq!(ban.reason, "griefing");
    assert_eq!(ban.source, "Server");
    assert_eq!(ban.created, Timestamp::parse("2026-10-28 23:33:38 +0900")?);
    assert_eq!(ban.expires, None);
    assert!(lists.bans.is_banned(uuid));

    let ip_ban = lists.bans.ip_ban("203.0.113.9").expect("ip ban");
    assert_eq!(ip_ban.reason, "spam");
    assert_eq!(ip_ban.source, "Server");
    assert!(lists.bans.is_ip_banned("203.0.113.9"));

    assert_eq!(lists.usercache.uuid_for("TESTUSER"), Some(uuid));
    assert_eq!(lists.usercache.name_for(uuid).as_deref(), Some("TestUser"));

    // A clean flush must not rewrite anything.
    lists.flush()?;
    for (file, expected) in FIXTURES {
        assert_eq!(read(dir.path(), file), expected, "{file} was rewritten");
    }
    Ok(())
}

#[test]
fn dirty_mutations_round_trip_to_identical_bytes() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    write_fixtures(dir.path());
    let clock = TestClock::new("2027-04-06 00:00:00 +0900");
    let lists = AdminLists::with_clock(dir.path(), clock)?;
    let uuid = Uuid::parse_str(TEST_USER)?;
    let temp = Uuid::parse_str("00000000-0000-0000-0000-000000000001")?;

    // Touch every file and then restore its original contents.
    lists.ops.add_op(temp, "Temp", 0, false);
    assert!(lists.ops.remove_op(temp));
    lists.whitelist.add(temp, "Temp");
    assert!(lists.whitelist.remove(temp));
    lists.bans.ban(temp, "Temp", "test", "Server", None);
    assert!(lists.bans.unban(temp));
    lists.bans.ban_ip("198.51.100.7", "test", "Server", None);
    assert!(lists.bans.unban_ip("198.51.100.7"));
    lists.usercache.cache("TestUser", uuid);
    lists.flush()?;

    for (file, expected) in FIXTURES {
        assert_eq!(
            read(dir.path(), file),
            expected,
            "{file} changed on round-trip"
        );
    }
    Ok(())
}

#[test]
fn usercache_is_compact_with_multiple_entries() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let clock = TestClock::new("2027-04-06 00:00:00 +0900");
    let lists = AdminLists::with_clock(dir.path(), clock)?;

    lists
        .usercache
        .cache("TestUser", Uuid::parse_str(TEST_USER)?);
    lists
        .usercache
        .cache("SecondUser", Uuid::parse_str(SECOND_USER)?);
    lists.flush()?;

    assert_eq!(read(dir.path(), "usercache.json"), USERCACHE_TWO_USERS_REF);
    Ok(())
}
