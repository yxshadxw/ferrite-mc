//! Mob behaviour: vanilla `LivingEntity.travel` physics plus the first two goals of the
//! passive animals, wander and look-at-player.
//!
//! - **Wander**: every 40 to 120 ticks the goal picks a random direction and walks for that
//!   interval, or idles for 20 to 60 ticks. The mob accelerates along its yaw and jumps over a
//!   block it walked into, like vanilla's `WaterAvoidingRandomStrollGoal`.
//! - **Look at player**: the nearest player within eight blocks turns the mob's body, head and
//!   pitch toward the player, as vanilla's `LookAtPlayerGoal`.
//!
//! Movement keeps the vanilla gravity, air drag and ground friction, tracks fall distance and
//! applies fall damage on landing, then runs the mob's despawn rules.

use crate::mob;
use crate::physics::{self, BlockGetter};
use crate::rng::Rng;
use crate::store::{EntityStore, Flags};
use ferrite_math::Vec3;

/// Vanilla `LivingEntity` gravity per tick.
pub const GRAVITY: f64 = 0.08;
/// Horizontal drag in the air (`0.91`).
pub const AIR_FRICTION: f64 = 0.91;
/// Vertical drag in the air (`0.98`).
pub const AIR_DRAG: f64 = 0.98;
/// Default block slipperiness.
pub const GROUND_FRICTION: f64 = 0.6;
/// Vanilla `LookAtPlayerGoal` range in blocks.
pub const LOOK_DISTANCE: f64 = 8.0;
/// Eye height of a player, added to the player's feet.
pub const PLAYER_EYE_HEIGHT: f64 = 1.62;
/// Share of the movement speed used as per-tick acceleration (friction caps terminal speed).
const WALK_ACCELERATION: f64 = 0.09;

/// Wraps an angle into `[-180, 180)`, like vanilla `Mth.wrapDegrees`.
pub fn wrap_degrees(angle: f32) -> f32 {
    let mut wrapped = angle % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    } else if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

/// The body yaw and pitch that make a mob at `pos` look at the nearest player within
/// [`LOOK_DISTANCE`], if any. `eye_height` is the mob's own eye offset above its feet.
pub fn look_at(pos: Vec3, eye_height: f64, players: &[Vec3]) -> Option<(f32, f32)> {
    let eye = pos.y + eye_height;
    let mut best: Option<(f64, Vec3)> = None;
    for player in players {
        let dx = player.x - pos.x;
        let dz = player.z - pos.z;
        let horizontal = (dx * dx + dz * dz).sqrt();
        if horizontal > LOOK_DISTANCE || (dx == 0.0 && dz == 0.0 && player.y == eye) {
            continue;
        }
        if best.is_none_or(|(distance, _)| horizontal < distance) {
            best = Some((horizontal, *player));
        }
    }
    let (horizontal, target) = best?;
    let dx = target.x - pos.x;
    let dz = target.z - pos.z;
    let dy = target.y + PLAYER_EYE_HEIGHT - eye;
    let yaw = if dx == 0.0 && dz == 0.0 {
        0.0
    } else {
        f64::from(wrap_degrees(dz.atan2(dx).to_degrees() as f32 - 90.0))
    };
    let pitch = -(dy.atan2(horizontal)).to_degrees();
    Some((yaw as f32, wrap_degrees(pitch as f32)))
}

/// Advances every mob's clocks, goals and movement, applies fall damage and runs despawn rules.
pub fn tick(store: &mut EntityStore, blocks: &impl BlockGetter, players: &[Vec3], rng: &mut Rng) {
    for key in store.keys().to_vec() {
        if !mob::is_mob(store, key) {
            continue;
        }
        mob::tick_clock(store, key);
        let Some(kind) = mob::state(store, key).map(|state| state.kind) else {
            continue;
        };
        let start = store.pos(key).unwrap_or_default();
        let flags = store.flags(key).unwrap_or_default();
        let was_on_ground = flags.has(Flags::ON_GROUND);

        // Wander goal: pick a new interval now and then.
        let mut walking = None;
        if let Some(mob) = mob::state_mut(store, key) {
            if mob.wander_timer == 0 {
                if rng.next_int(4) == 0 {
                    mob.moving = false;
                    mob.wander_timer = 20 + rng.next_int(40) as u32;
                } else {
                    mob.moving = true;
                    mob.wander_yaw = rng.next_f32() * 360.0;
                    mob.wander_timer = 40 + rng.next_int(80) as u32;
                }
            } else {
                mob.wander_timer -= 1;
            }
            if mob.moving {
                walking = Some(mob.wander_yaw);
            }
        }

        // Look at player: wins over the walking direction for body, head and pitch.
        let look = look_at(start, f64::from(kind.props().eye_height), players);
        if let Some((yaw, pitch)) = look
            && let Some(data) = store.data_mut(key)
        {
            data.yaw = yaw;
            data.head_yaw = yaw;
            data.pitch = pitch;
        }

        let mut vel = store.vel(key).unwrap_or_default();
        if let Some(yaw) = walking {
            let acceleration = kind.movement_speed() * WALK_ACCELERATION;
            let (sin, cos) = f64::from(yaw).to_radians().sin_cos();
            vel.x -= sin * acceleration;
            vel.z += cos * acceleration;
            if look.is_none()
                && let Some(data) = store.data_mut(key)
            {
                data.yaw = yaw;
                data.head_yaw = yaw;
            }
            // Jump over a block walked into last tick.
            if was_on_ground && flags.has(Flags::HORIZONTAL_COLLISION) {
                vel.y = kind.jump_strength();
            }
        }
        if !was_on_ground {
            vel.y -= GRAVITY;
        }
        store.set_vel(key, vel);
        physics::move_entity(store, key, vel, blocks);

        // Vertical drag and the friction of the block the mob now stands on.
        let on_ground = store
            .flags(key)
            .is_some_and(|flags| flags.has(Flags::ON_GROUND));
        let mut vel = store.vel(key).unwrap_or_default();
        let friction = if on_ground { GROUND_FRICTION } else { 1.0 };
        vel.x *= AIR_FRICTION * friction;
        vel.z *= AIR_FRICTION * friction;
        if !on_ground {
            vel.y *= AIR_DRAG;
        }
        store.set_vel(key, vel);

        // Fall distance grows while airborne and hurts on the landing tick.
        let end = store.pos(key).unwrap_or_default();
        let fallen = (start.y - end.y).max(0.0) as f32;
        let Some(mob) = mob::state_mut(store, key) else {
            continue;
        };
        if on_ground {
            let distance = mob.fall_distance + fallen;
            mob.fall_distance = 0.0;
            if distance > 0.0 {
                mob::apply_fall_damage(store, key, distance);
            }
        } else {
            mob.fall_distance += fallen;
        }
        if mob::is_mob(store, key) {
            mob::check_despawn(store, key, players, false, rng);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mob::{MobKind, new_mob};
    use crate::physics::tests::TestWorld;

    fn cow(x: f64, y: f64, z: f64) -> (EntityStore, crate::EntityKey) {
        let mut store = EntityStore::default();
        let key = store.insert(new_mob(MobKind::Cow, Vec3::new(x, y, z), &mut Rng::new(1)));
        (store, key)
    }

    #[test]
    fn gravity_lands_a_mob_on_the_ground() {
        let world = TestWorld::flat(64);
        let (mut store, key) = cow(0.5, 70.0, 0.5);
        let mut rng = Rng::new(2);
        for _ in 0..80 {
            tick(&mut store, &world, &[], &mut rng);
        }
        let pos = store.pos(key).unwrap();
        assert!((pos.y - 64.0).abs() < 1e-9, "rests on the floor: {pos:?}");
        assert!(store.flags(key).unwrap().has(Flags::ON_GROUND));
        assert_eq!(store.vel(key).unwrap().y, 0.0);
        assert_eq!(mob::state(&store, key).unwrap().fall_distance, 0.0);
    }

    #[test]
    fn a_long_fall_kills_a_cow() {
        let world = TestWorld::flat(64);
        let (mut store, key) = cow(0.5, 100.5, 0.5);
        let mut rng = Rng::new(3);
        for _ in 0..120 {
            tick(&mut store, &world, &[], &mut rng);
            if !mob::is_mob(&store, key) {
                break;
            }
        }
        assert!(!mob::is_mob(&store, key));
        assert!(store.flags(key).unwrap().has(Flags::REMOVED));
    }

    #[test]
    fn a_short_drop_hurts_but_does_not_kill() {
        let world = TestWorld::flat(64);
        let (mut store, key) = cow(0.5, 68.0, 0.5);
        let mut rng = Rng::new(4);
        for _ in 0..60 {
            tick(&mut store, &world, &[], &mut rng);
        }
        let health = mob::state(&store, key).unwrap().health;
        assert!((health - 9.0).abs() < 1e-6, "four-block fall: {health}");
    }

    #[test]
    fn despawn_rules_run_every_tick() {
        let world = TestWorld::flat(64);
        let (mut store, key) = cow(0.5, 64.0, 0.5);
        mob::state_mut(&mut store, key).unwrap().despawns = true;
        let mut rng = Rng::new(5);
        tick(
            &mut store,
            &world,
            &[Vec3::new(1000.0, 64.0, 1000.0)],
            &mut rng,
        );
        assert!(store.flags(key).unwrap().has(Flags::REMOVED));
    }

    #[test]
    fn wander_walks_along_the_chosen_yaw() {
        let world = TestWorld::flat(64);
        let (mut store, key) = cow(0.5, 64.0, 0.5);
        {
            let mob = mob::state_mut(&mut store, key).unwrap();
            mob.moving = true;
            mob.wander_timer = 200;
            mob.wander_yaw = 0.0;
        }
        let mut rng = Rng::new(6);
        for _ in 0..40 {
            tick(&mut store, &world, &[], &mut rng);
        }
        let pos = store.pos(key).unwrap();
        // Yaw zero looks south, +z.
        assert!(pos.z > 1.0, "walked south: {pos:?}");
        assert!((pos.y - 64.0).abs() < 1e-9);
    }

    #[test]
    fn a_nearby_player_is_looked_at() {
        let mob = Vec3::new(0.5, 64.0, 0.5);
        let (yaw, _) = look_at(
            mob,
            f64::from(MobKind::Cow.props().eye_height),
            &[Vec3::new(4.5, 64.0, 0.5)],
        )
        .expect("player within eight blocks");
        assert!((yaw + 90.0).abs() < 1e-3, "faces east: {yaw}");
        assert!(
            look_at(
                mob,
                f64::from(MobKind::Cow.props().eye_height),
                &[Vec3::new(9.5, 64.0, 0.5)]
            )
            .is_none()
        );
    }
}
