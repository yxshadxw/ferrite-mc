//! Mob movement. Vanilla `LivingEntity.travel` without AI input yet: gravity, air drag and
//! ground friction, collision against block shapes, fall distance and fall damage. The goal
//! behaviours (wander, look at player) add the horizontal input on top of this.

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

/// Advances every mob's clocks and movement, applies fall damage and runs despawn rules.
pub fn tick(store: &mut EntityStore, blocks: &impl BlockGetter, players: &[Vec3], rng: &mut Rng) {
    for key in store.keys().to_vec() {
        if !mob::is_mob(store, key) {
            continue;
        }
        mob::tick_clock(store, key);
        let was_on_ground = store
            .flags(key)
            .is_some_and(|flags| flags.has(Flags::ON_GROUND));
        let start = store.pos(key).unwrap_or_default();
        let mut vel = store.vel(key).unwrap_or_default();
        if !was_on_ground {
            vel.y -= GRAVITY;
            store.set_vel(key, vel);
        }
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
}
