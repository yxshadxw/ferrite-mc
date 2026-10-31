//! Natural spawning of the passive animals in loaded chunks.
//!
//! Follows vanilla `NaturalSpawner` for the creature category in the essentials: creatures
//! spawn at the surface on grass in daylight (light level above eight), at least 24 blocks
//! from every player and within 128 blocks of one, in a herd of one to three, and only while
//! the creature mob cap of ten per player is not reached. The world supplies blocks and light
//! through [`SpawnEnv`]; the caller inserts the returned records into its region store.

use crate::mob::{MobKind, new_mob};
use crate::physics::BlockGetter;
use crate::rng::Rng;
use crate::store::{EntityRecord, EntityStore, Flags};
use ferrite_math::Vec3;

/// Vanilla `MobCategory.CREATURE` mob cap, per player.
pub const CREATURE_CAP_PER_PLAYER: usize = 10;
/// Vanilla minimum spawn distance from a player.
pub const MIN_SPAWN_DISTANCE: f64 = 24.0;
/// Vanilla maximum spawn distance (also the creature despawn distance).
pub const MAX_SPAWN_DISTANCE: f64 = 128.0;
/// Vanilla animals spawn where the light level is above eight.
pub const MIN_LIGHT: u8 = 9;
/// Highest block the surface search looks at.
const MAX_SURFACE_Y: i32 = 96;
/// Lowest block the surface search looks at.
const MIN_SURFACE_Y: i32 = -64;

/// The block and light queries natural spawning needs.
pub trait SpawnEnv: BlockGetter {
    /// The light level at a block, as vanilla's `getMaxLocalRawBrightness`.
    fn light(&self, x: i32, y: i32, z: i32) -> u8;
}

/// Number of live modelled mobs in a store.
pub fn creature_count(store: &EntityStore) -> usize {
    store
        .keys()
        .iter()
        .filter(|key| {
            store.data(**key).is_some_and(|data| data.mob.is_some())
                && store
                    .flags(**key)
                    .is_some_and(|flags| !flags.has(Flags::REMOVED))
        })
        .count()
}

/// Attempts `attempts` natural spawns and returns the new mobs; the caller inserts them.
///
/// With no players there is nothing to spawn around, and a full creature mob cap stops the
/// attempts before any block lookups.
pub fn natural_spawn(
    store: &EntityStore,
    env: &impl SpawnEnv,
    players: &[Vec3],
    rng: &mut Rng,
    attempts: usize,
) -> Vec<EntityRecord> {
    if players.is_empty() {
        return Vec::new();
    }
    let cap = CREATURE_CAP_PER_PLAYER * players.len();
    if creature_count(store) >= cap {
        return Vec::new();
    }
    let mut out = Vec::new();
    for _ in 0..attempts {
        if creature_count(store) + out.len() >= cap {
            break;
        }
        let player = &players[rng.next_int(players.len() as i32) as usize];
        let angle = rng.next_f64() * std::f64::consts::TAU;
        let distance = rng.range(MIN_SPAWN_DISTANCE, MAX_SPAWN_DISTANCE);
        let x = (player.x + angle.cos() * distance).floor() as i32;
        let z = (player.z + angle.sin() * distance).floor() as i32;
        let Some(y) = surface(env, x, z) else {
            continue;
        };
        if !can_spawn(env, x, y, z) || !far_enough(players, x, z) {
            continue;
        }
        let kind = MobKind::ALL[rng.next_int(MobKind::ALL.len() as i32) as usize];
        // Vanilla herds: one to three animals, spread over the neighbouring blocks.
        for _ in 0..1 + rng.next_int(3) {
            let ox = x + rng.next_int(3) - 1;
            let oz = z + rng.next_int(3) - 1;
            let oy = surface(env, ox, oz).unwrap_or(y);
            if !can_spawn(env, ox, oy, oz) || !far_enough(players, ox, oz) {
                continue;
            }
            let pos = Vec3::new(f64::from(ox) + 0.5, f64::from(oy), f64::from(oz) + 0.5);
            out.push(new_mob(kind, pos, rng));
            if creature_count(store) + out.len() >= cap {
                break;
            }
        }
    }
    out
}

/// Whether a spawn column is at least 24 blocks from every player and within 128 of one.
fn far_enough(players: &[Vec3], x: i32, z: i32) -> bool {
    let cx = f64::from(x) + 0.5;
    let cz = f64::from(z) + 0.5;
    let mut too_close = false;
    let mut in_range = false;
    for player in players {
        let dx = player.x - cx;
        let dz = player.z - cz;
        let distance = dx * dx + dz * dz;
        too_close |= distance < MIN_SPAWN_DISTANCE * MIN_SPAWN_DISTANCE;
        in_range |= distance <= MAX_SPAWN_DISTANCE * MAX_SPAWN_DISTANCE;
    }
    !too_close && in_range
}

/// The terrain height natural spawning would use: one above the first non-air block below
/// [`MAX_SURFACE_Y`], or `None` for an unloaded or empty column.
pub fn surface(env: &impl SpawnEnv, x: i32, z: i32) -> Option<i32> {
    for y in (MIN_SURFACE_Y..=MAX_SURFACE_Y).rev() {
        let state = env.block_state(x, y, z)?;
        if !is_air(state) {
            return Some(y + 1);
        }
    }
    None
}

fn is_air(state: u16) -> bool {
    usize::from(state) < ferrite_data::blocks::STATE_COUNT
        && ferrite_data::block_props::get(state).is_air
}

fn is_grass(state: u16) -> bool {
    ferrite_data::blocks::state(state).is_some_and(|(name, _)| name == "minecraft:grass_block")
}

/// Vanilla `Animal.checkAnimalSpawnRules` and `isSpawnablePosition`: grass below, two air
/// blocks above and light above eight.
pub fn can_spawn(env: &impl SpawnEnv, x: i32, y: i32, z: i32) -> bool {
    let (Some(below), Some(feet), Some(head)) = (
        env.block_state(x, y - 1, z),
        env.block_state(x, y, z),
        env.block_state(x, y + 1, z),
    ) else {
        return false;
    };
    is_grass(below) && is_air(feet) && is_air(head) && env.light(x, y, z) >= MIN_LIGHT
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::BlockGetter;

    struct Env {
        light: u8,
        floor: &'static str,
    }

    impl BlockGetter for Env {
        fn block_state(&self, _x: i32, y: i32, _z: i32) -> Option<u16> {
            let name = if y == 63 {
                self.floor
            } else if y < 63 {
                "stone"
            } else {
                "air"
            };
            Some(ferrite_data::blocks::default_state(name).unwrap())
        }
    }

    impl SpawnEnv for Env {
        fn light(&self, _x: i32, _y: i32, _z: i32) -> u8 {
            self.light
        }
    }

    fn player() -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }

    #[test]
    fn spawns_grass_animals_in_daylight_within_the_cap() {
        let env = Env {
            light: 15,
            floor: "grass_block",
        };
        let store = EntityStore::default();
        let mut rng = Rng::new(11);
        let spawned = natural_spawn(&store, &env, &[player()], &mut rng, 64);
        assert!(!spawned.is_empty(), "a daylight meadow has animals");
        for record in &spawned {
            assert_eq!(record.pos.y.floor(), 64.0);
            assert!(record.data.mob.is_some());
            let dx = record.pos.x - player().x;
            let dz = record.pos.z - player().z;
            let distance = (dx * dx + dz * dz).sqrt();
            assert!(
                (MIN_SPAWN_DISTANCE..=MAX_SPAWN_DISTANCE + 2.0).contains(&distance),
                "spawned {distance} blocks from the player"
            );
        }
    }

    #[test]
    fn darkness_stone_and_an_absent_player_never_spawn() {
        let mut rng = Rng::new(12);
        let store = EntityStore::default();
        let dark = Env {
            light: 7,
            floor: "grass_block",
        };
        assert!(natural_spawn(&store, &dark, &[player()], &mut rng, 64).is_empty());
        let stone = Env {
            light: 15,
            floor: "stone",
        };
        assert!(natural_spawn(&store, &stone, &[player()], &mut rng, 64).is_empty());
        assert!(natural_spawn(&store, &stone, &[], &mut rng, 64).is_empty());
    }

    #[test]
    fn the_creature_mob_cap_stops_spawning() {
        let env = Env {
            light: 15,
            floor: "grass_block",
        };
        let mut store = EntityStore::default();
        let mut rng = Rng::new(13);
        for _ in 0..CREATURE_CAP_PER_PLAYER {
            store.insert(new_mob(MobKind::Pig, Vec3::new(0.5, 64.0, 0.5), &mut rng));
        }
        assert_eq!(creature_count(&store), CREATURE_CAP_PER_PLAYER);
        assert!(natural_spawn(&store, &env, &[player()], &mut rng, 64).is_empty());
        // A second player raises the cap, so the meadow is used again.
        let other = Vec3::new(64.5, 64.0, 64.5);
        assert!(!natural_spawn(&store, &env, &[player(), other], &mut rng, 64).is_empty());
    }

    #[test]
    fn surface_finds_the_first_open_block() {
        let env = Env {
            light: 15,
            floor: "grass_block",
        };
        assert_eq!(surface(&env, 0, 0), Some(64));
        // A column with a roof above the surface still spawns on the surface below it; the
        // light rule then rejects the covered spot.
        struct Roof;
        impl BlockGetter for Roof {
            fn block_state(&self, _x: i32, y: i32, _z: i32) -> Option<u16> {
                let name = match y {
                    80 => "stone",
                    y if y < 63 => "stone",
                    _ => "air",
                };
                Some(ferrite_data::blocks::default_state(name).unwrap())
            }
        }
        impl SpawnEnv for Roof {
            fn light(&self, _x: i32, _y: i32, _z: i32) -> u8 {
                7
            }
        }
        assert_eq!(surface(&Roof, 0, 0), Some(81));
        assert!(!can_spawn(&Roof, 0, 81, 0));
    }
}
