//! Behaviour tests for the public admin-list API.

mod common;

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use common::TestClock;
use ferrite_admin::{AdminLists, Timestamp};
use uuid::Uuid;

const TEST_USER: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
const SECOND_USER: &str = "8667ba71-b85a-4004-af54-457a9734eed7";
const TEMP_USER: &str = "00000000-0000-0000-0000-000000000001";

const OPS_REF: &str = r#"[
  {
    "uuid": "069a79f4-44e9-4726-a5be-fca90e38aaf5",
    "name": "TestUser",
    "level": 4,
    "bypassesPlayerLimit": false
  }
]"#;

const USERCACHE_STALE: &str = r#"[{"uuid":"069a79f4-44e9-4726-a5be-fca90e38aaf5","name":"TestUser","expiresOn":"2027-06-01 00:00:00 +0900"}]"#;

fn read(dir: &Path, file: &str) -> String {
    fs::read_to_string(dir.join(file)).unwrap_or_else(|error| panic!("reading {file}: {error}"))
}

#[test]
fn ops_upsert_and_remove() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let lists = AdminLists::load(dir.path())?;
    let uuid = Uuid::parse_str(TEST_USER)?;

    assert_eq!(lists.ops.level_of(uuid), None);
    lists.add_op(uuid, "TestUser", 3, true);
    assert_eq!(lists.ops.level_of(uuid), Some(3));

    lists.ops.add_op(uuid, "TestUser", 4, false);
    assert_eq!(lists.ops.level_of(uuid), Some(4));

    assert!(lists.remove_op(uuid));
    assert_eq!(lists.ops.level_of(uuid), None);
    assert!(!lists.remove_op(uuid));
    Ok(())
}

#[test]
fn whitelist_names_ignore_case() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let lists = AdminLists::load(dir.path())?;
    let uuid = Uuid::parse_str(TEST_USER)?;

    lists.whitelist.add(uuid, "Alice");
    assert!(lists.whitelist.contains(uuid));
    assert!(lists.whitelist.contains_name("alice"));
    assert!(lists.whitelist.contains_name("ALICE"));
    assert!(!lists.whitelist.contains_name("Bob"));

    lists.whitelist.add(uuid, "Alicia");
    assert!(!lists.whitelist.contains_name("Alice"));
    assert!(lists.whitelist.contains_name("alicia"));

    assert!(lists.whitelist.remove_name("ALICIA"));
    assert!(!lists.whitelist.contains(uuid));
    assert!(!lists.whitelist.remove_name("Alicia"));
    Ok(())
}

#[test]
fn usercache_names_ignore_case_and_replace() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let clock = TestClock::new("2026-01-01 00:00:00 +0000");
    let lists = AdminLists::with_clock(dir.path(), clock)?;
    let alice = Uuid::parse_str(TEST_USER)?;
    let bob = Uuid::parse_str(SECOND_USER)?;

    lists.usercache.cache("Alice", alice);
    assert_eq!(lists.usercache.uuid_for("alice"), Some(alice));
    assert_eq!(lists.usercache.uuid_for("ALICE"), Some(alice));
    assert_eq!(lists.usercache.name_for(alice).as_deref(), Some("Alice"));

    lists.usercache.cache("ALICE", bob);
    assert_eq!(lists.usercache.uuid_for("Alice"), Some(bob));
    assert_eq!(lists.usercache.name_for(alice), None);
    assert_eq!(lists.usercache.name_for(bob).as_deref(), Some("ALICE"));
    Ok(())
}

#[test]
fn player_bans_honor_expiry() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let clock = TestClock::new("2026-10-28 12:00:00 +0900");
    let lists = AdminLists::with_clock(dir.path(), clock.clone())?;
    let uuid = Uuid::parse_str(TEST_USER)?;
    let expires = Timestamp::parse("2026-10-28 13:00:00 +0900")?;

    lists
        .bans
        .ban(uuid, "Alice", "spam", "Server", Some(expires));
    let ban = lists.bans.player_ban(uuid).expect("active ban");
    assert_eq!(ban.reason, "spam");
    assert_eq!(ban.source, "Server");
    assert_eq!(ban.created, Timestamp::parse("2026-10-28 12:00:00 +0900")?);
    assert_eq!(ban.expires, Some(expires));
    assert!(lists.bans.is_banned(uuid));

    // Vanilla considers a ban expired only once "now" is past the expiry.
    clock.set("2026-10-28 13:00:00 +0900");
    assert!(lists.bans.is_banned(uuid));

    clock.set("2026-10-28 13:00:01 +0900");
    assert!(!lists.bans.is_banned(uuid));
    assert!(lists.bans.player_ban(uuid).is_none());
    assert!(lists.bans.player_bans().is_empty());

    lists.flush()?;
    assert_eq!(read(dir.path(), "banned-players.json"), "[]");
    Ok(())
}

#[test]
fn ip_bans_honor_expiry_and_case() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let clock = TestClock::new("2026-10-28 12:00:00 +0900");
    let lists = AdminLists::with_clock(dir.path(), clock.clone())?;
    let expires = Timestamp::parse("2026-10-28 13:00:00 +0900")?;

    lists
        .bans
        .ban_ip("2001:db8::1", "proxy abuse", "Server", Some(expires));
    let ban = lists.bans.ip_ban("2001:DB8::1").expect("active ip ban");
    assert_eq!(ban.reason, "proxy abuse");
    assert_eq!(ban.source, "Server");
    assert!(lists.bans.is_ip_banned("2001:DB8::1"));

    clock.set("2026-10-28 13:00:00 +0900");
    assert!(lists.bans.is_ip_banned("2001:db8::1"));

    clock.set("2026-10-28 13:00:01 +0900");
    assert!(!lists.bans.is_ip_banned("2001:db8::1"));
    assert!(lists.bans.ip_bans().is_empty());

    lists.flush()?;
    assert_eq!(read(dir.path(), "banned-ips.json"), "[]");
    Ok(())
}

#[test]
fn forever_bans_never_expire() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let clock = TestClock::new("2026-10-28 12:00:00 +0900");
    let lists = AdminLists::with_clock(dir.path(), clock.clone())?;
    let uuid = Uuid::parse_str(TEST_USER)?;

    lists.bans.ban(uuid, "Alice", "griefing", "Server", None);
    clock.set("2099-01-01 00:00:00 +0900");
    assert!(lists.bans.is_banned(uuid));
    assert_eq!(lists.bans.player_ban(uuid).unwrap().expires, None);
    Ok(())
}

#[test]
fn usercache_expires_after_eight_weeks() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let clock = TestClock::new("2026-01-01 00:00:00 +0000");
    let lists = AdminLists::with_clock(dir.path(), clock.clone())?;
    let uuid = Uuid::parse_str(TEST_USER)?;

    lists.usercache.cache("Alice", uuid);
    lists.flush()?;
    assert_eq!(
        read(dir.path(), "usercache.json"),
        r#"[{"uuid":"069a79f4-44e9-4726-a5be-fca90e38aaf5","name":"Alice","expiresOn":"2026-02-26 00:00:00 +0000"}]"#
    );

    // Exactly eight weeks later the entry is still valid; a second later it is not.
    clock.set("2026-02-26 00:00:00 +0000");
    assert_eq!(lists.usercache.uuid_for("alice"), Some(uuid));
    assert_eq!(lists.usercache.name_for(uuid).as_deref(), Some("Alice"));

    clock.set("2026-02-26 00:00:01 +0000");
    assert_eq!(lists.usercache.uuid_for("alice"), None);
    assert_eq!(lists.usercache.name_for(uuid), None);

    lists.flush()?;
    assert_eq!(read(dir.path(), "usercache.json"), "[]");
    Ok(())
}

#[test]
fn dirty_tracking_writes_only_changed_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let lists = AdminLists::load(dir.path())?;
    lists.flush()?;
    for file in [
        "ops.json",
        "whitelist.json",
        "banned-players.json",
        "banned-ips.json",
        "usercache.json",
    ] {
        assert!(!dir.path().join(file).exists(), "clean flush wrote {file}");
    }

    lists
        .ops
        .add_op(Uuid::parse_str(TEST_USER)?, "TestUser", 4, false);
    lists.flush()?;
    assert_eq!(read(dir.path(), "ops.json"), OPS_REF);
    for file in [
        "whitelist.json",
        "banned-players.json",
        "banned-ips.json",
        "usercache.json",
    ] {
        assert!(
            !dir.path().join(file).exists(),
            "ops-only flush wrote {file}"
        );
    }
    Ok(())
}

#[test]
fn atomic_save_ignores_leftover_temp_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let uuid = Uuid::parse_str(TEST_USER)?;
    let temp = Uuid::parse_str(TEMP_USER)?;

    // A crash leftover must not shadow the real file.
    fs::write(dir.path().join("ops.json"), OPS_REF)?;
    fs::write(dir.path().join("ops.json.tmp"), "not json")?;
    let lists = AdminLists::load(dir.path())?;
    assert_eq!(lists.ops.level_of(uuid), Some(4));

    lists.ops.add_op(temp, "Temp", 0, false);
    lists.flush()?;
    assert!(!dir.path().join("ops.json.tmp").exists());

    let reloaded = AdminLists::load(dir.path())?;
    assert_eq!(reloaded.ops.level_of(uuid), Some(4));
    assert_eq!(reloaded.ops.level_of(temp), Some(0));

    // A temp file with no final file at all is ignored, not loaded.
    fs::write(dir.path().join("usercache.json.tmp"), USERCACHE_STALE)?;
    let reloaded = AdminLists::load(dir.path())?;
    assert_eq!(reloaded.usercache.uuid_for("TestUser"), None);
    Ok(())
}

#[test]
fn concurrent_readers_and_writer() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let lists = Arc::new(AdminLists::load(dir.path())?);
    let alice = Uuid::parse_str(TEST_USER)?;
    let bob = Uuid::parse_str(SECOND_USER)?;

    lists.ops.add_op(alice, "Alice", 4, false);
    lists.whitelist.add(alice, "Alice");
    lists.usercache.cache("Alice", alice);

    let stop = Arc::new(AtomicBool::new(false));
    let mut readers = Vec::new();
    for _ in 0..4 {
        let lists = Arc::clone(&lists);
        let stop = Arc::clone(&stop);
        readers.push(thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                assert_eq!(lists.ops.level_of(alice), Some(4));
                assert!(lists.whitelist.contains(alice));
                assert!(lists.whitelist.contains_name("ALICE"));
                assert_eq!(lists.usercache.uuid_for("alice"), Some(alice));
                assert_eq!(lists.usercache.name_for(alice).as_deref(), Some("Alice"));
                assert!(!lists.bans.is_banned(alice));
            }
        }));
    }

    let writer = {
        let lists = Arc::clone(&lists);
        thread::spawn(move || {
            for index in 0..200 {
                lists.ops.add_op(bob, "Bob", 1, index % 2 == 0);
                lists.whitelist.add(bob, "Bob");
                lists.bans.ban(bob, "Bob", "test", "Server", None);
                assert!(lists.bans.unban(bob));
                lists.usercache.cache("Bob", bob);
                assert!(lists.whitelist.remove(bob));
                if index % 50 == 0 {
                    lists.flush().expect("flush");
                }
            }
        })
    };

    writer.join().expect("writer thread");
    stop.store(true, Ordering::Release);
    for reader in readers {
        reader.join().expect("reader thread");
    }

    lists.flush()?;
    let reloaded = AdminLists::load(dir.path())?;
    assert_eq!(reloaded.ops.level_of(alice), Some(4));
    assert!(reloaded.whitelist.contains_name("Alice"));
    assert_eq!(reloaded.usercache.uuid_for("Bob"), Some(bob));
    assert_eq!(reloaded.usercache.name_for(alice).as_deref(), Some("Alice"));
    Ok(())
}
