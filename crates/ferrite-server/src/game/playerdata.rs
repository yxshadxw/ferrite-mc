//! Per-player save data so a player who leaves resumes where they were instead of at world spawn.
//!
//! Stored as `<world>/playerdata/<uuid>.json` (or `./playerdata` without a world) and written
//! atomically (temp file + rename) when the player leaves. A vanilla NBT `.dat` compatible format can
//! replace this later without touching callers.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ferrite_protocol::packets::play::ItemStack;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Set the save directory once at startup. Later calls are ignored.
pub fn init(dir: PathBuf) {
    let _ = DIR.set(dir);
}

fn dir() -> &'static Path {
    DIR.get().map_or(Path::new("playerdata"), PathBuf::as_path)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub gamemode: i32,
    /// `(slot, item id, count)` for non-empty slots only.
    pub inventory: Vec<(u8, i32, i32)>,
}

impl Saved {
    pub fn capture(
        pos: [f64; 3],
        yaw: f32,
        pitch: f32,
        gamemode: i32,
        inventory: &[ItemStack],
    ) -> Self {
        let inventory = inventory
            .iter()
            .enumerate()
            .filter(|(_, stack)| stack.count > 0)
            .map(|(slot, stack)| (slot as u8, stack.item_id, stack.count))
            .collect();
        Self {
            pos,
            yaw,
            pitch,
            gamemode,
            inventory,
        }
    }

    /// Rebuild the 46-slot window-0 inventory, ignoring slots outside the window.
    pub fn inventory_array(&self) -> [ItemStack; 46] {
        let mut slots = [ItemStack::default(); 46];
        for &(slot, item_id, count) in &self.inventory {
            if let Some(target) = slots.get_mut(usize::from(slot)) {
                *target = ItemStack { count, item_id };
            }
        }
        slots
    }
}

fn path_in(dir: &Path, uuid: &Uuid) -> PathBuf {
    dir.join(format!("{uuid}.json"))
}

pub fn load_from(dir: &Path, uuid: &Uuid) -> Option<Saved> {
    let bytes = std::fs::read(path_in(dir, uuid)).ok()?;
    match serde_json::from_slice(&bytes) {
        Ok(saved) => Some(saved),
        Err(error) => {
            tracing::warn!(%uuid, %error, "ignoring unreadable player data");
            None
        }
    }
}

pub fn save_to(dir: &Path, uuid: &Uuid, saved: &Saved) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let target = path_in(dir, uuid);
    let temp = target.with_extension("json.tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(saved).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(temp, target)
}

pub fn load(uuid: &Uuid) -> Option<Saved> {
    load_from(dir(), uuid)
}

pub fn save(uuid: &Uuid, saved: &Saved) {
    if let Err(error) = save_to(dir(), uuid, saved) {
        tracing::error!(%uuid, %error, "failed to save player data");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-playerdata-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn round_trips_position_mode_and_inventory() {
        let dir = temp_dir("roundtrip");
        let uuid = Uuid::from_u128(7);
        let mut inventory = [ItemStack::default(); 46];
        inventory[36] = ItemStack {
            count: 64,
            item_id: 1,
        };
        inventory[44] = ItemStack {
            count: 3,
            item_id: 99,
        };
        let saved = Saved::capture([12.5, 70.0, -3.25], 90.0, -15.0, 1, &inventory);
        save_to(&dir, &uuid, &saved).unwrap();
        let loaded = load_from(&dir, &uuid).unwrap();
        assert_eq!(loaded, saved);
        assert_eq!(loaded.inventory_array()[36].count, 64);
        assert_eq!(loaded.inventory_array()[44].item_id, 99);
        assert_eq!(loaded.inventory_array()[0], ItemStack::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_or_corrupt_data_is_none() {
        let dir = temp_dir("corrupt");
        let uuid = Uuid::from_u128(8);
        assert!(load_from(&dir, &uuid).is_none());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(path_in(&dir, &uuid), b"{ not json").unwrap();
        assert!(load_from(&dir, &uuid).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn out_of_range_slots_are_ignored() {
        let saved = Saved {
            pos: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            gamemode: 0,
            inventory: vec![(200, 1, 1), (3, 2, 5)],
        };
        let slots = saved.inventory_array();
        assert_eq!(slots[3].count, 5);
        assert_eq!(slots.iter().filter(|s| s.count > 0).count(), 1);
    }
}
