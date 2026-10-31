//! Arrow projectiles (vanilla `AbstractArrow`): the flight physics, block collision and hit
//! detection used by skeleton bows and player bows.
//!
//! An arrow is an ordinary entity of type `minecraft:arrow` with an [`ArrowState`] carrying the
//! damage and owner; the tracker sends it like any other entity. [`tick_arrows`] advances every
//! arrow against the region's blocks and entities and reports player hits, which the region
//! applies because players are not in the entity store.

use crate::id::EntityKey;
use crate::mob;
use crate::physics::{self, BlockGetter};
use crate::rng::Rng;
use crate::store::{EntityData, EntityRecord, EntityStore, Flags};
use crate::{allocate_id, random_uuid};
use ferrite_math::{AABB, Vec3};
use std::sync::OnceLock;
use uuid::Uuid;

/// Vanilla `AbstractArrow` gravity per tick.
pub const GRAVITY: f64 = 0.05;
/// Vanilla arrow drag (`0.99`).
pub const DRAG: f64 = 0.99;
/// Vanilla `AbstractArrow` despawn age (`1200` ticks).
pub const LIFETIME: u32 = 1200;
/// Vanilla full-draw arrow speed.
pub const SPEED: f64 = 3.0;
pub const WIDTH: f64 = 0.5;
pub const HEIGHT: f64 = 0.5;
/// Ticks before an arrow can hit its shooter.
pub const OWNER_GRACE: u32 = 5;

/// Per-arrow state beside the entity columns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrowState {
    /// Damage on the next hit.
    pub damage: f32,
    /// Shooter UUID, so an arrow does not hit its owner immediately.
    pub owner: Option<Uuid>,
    pub critical: bool,
    pub age: u32,
}

/// Entity type id of `minecraft:arrow`.
pub fn arrow_type() -> u16 {
    static KIND: OnceLock<u16> = OnceLock::new();
    *KIND.get_or_init(|| {
        ferrite_data::entity_props::by_name("arrow")
            .expect("vanilla has an arrow entity type")
            .id
    })
}

/// A new arrow at `pos` flying with `vel`.
pub fn new_arrow(
    pos: Vec3,
    vel: Vec3,
    damage: f32,
    owner: Option<Uuid>,
    _rng: &mut Rng,
) -> EntityRecord {
    let mut data = EntityData::new(allocate_id(), random_uuid(), arrow_type(), WIDTH, HEIGHT);
    let horizontal = (vel.x * vel.x + vel.z * vel.z).sqrt();
    data.yaw = (-vel.x).atan2(vel.z).to_degrees() as f32;
    data.pitch = (-vel.y).atan2(horizontal).to_degrees() as f32;
    data.head_yaw = data.yaw;
    data.arrow = Some(ArrowState {
        damage,
        owner,
        critical: false,
        age: 0,
    });
    // Vanilla arrows do not randomise their start inside the block.
    EntityRecord {
        pos,
        vel,
        flags: Flags::default(),
        data,
    }
}

/// A player an arrow hit; the region applies the damage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerHit {
    pub uuid: Uuid,
    pub damage: f32,
}

fn remove(store: &mut EntityStore, key: EntityKey) {
    if let Some(flags) = store.flags_mut(key) {
        flags.set(Flags::REMOVED, true);
    }
}

/// Advances every arrow one tick: gravity, drag, block collision and entity/player hits.
///
/// Mobs in `store` are hurt directly (vanilla `AbstractArrow.onHitEntity`), players are
/// returned for the region to damage. An arrow that hits anything is removed.
pub fn tick_arrows(
    store: &mut EntityStore,
    blocks: &impl BlockGetter,
    players: &[(Uuid, AABB)],
) -> Vec<PlayerHit> {
    let keys = store.keys().to_vec();
    let mut hits = Vec::new();
    for key in keys {
        let Some(mut arrow) = store.data(key).and_then(|data| data.arrow) else {
            continue;
        };
        if store
            .flags(key)
            .is_some_and(|flags| flags.has(Flags::REMOVED))
        {
            continue;
        }
        arrow.age += 1;
        if arrow.age > LIFETIME {
            remove(store, key);
            continue;
        }
        let mut vel = store.vel(key).unwrap_or_default();
        vel.y -= GRAVITY;
        vel.x *= DRAG;
        vel.y *= DRAG;
        vel.z *= DRAG;
        store.set_vel(key, vel);
        physics::move_entity(store, key, vel, blocks);
        // An arrow stops in the first block it touches (vanilla sticks; removal is enough for
        // a region-owned projectile).
        if store.flags(key).is_some_and(|flags| {
            flags.has(Flags::HORIZONTAL_COLLISION) || flags.has(Flags::VERTICAL_COLLISION)
        }) {
            remove(store, key);
            continue;
        }
        let Some(bbox) = store.bbox(key) else {
            continue;
        };
        if arrow.age > OWNER_GRACE
            && let Some((uuid, _)) = players.iter().find(|(_, box_)| box_.intersects(bbox))
        {
            hits.push(PlayerHit {
                uuid: *uuid,
                damage: arrow.damage,
            });
            remove(store, key);
            continue;
        }
        let target = store
            .query_vec(bbox)
            .into_iter()
            .find(|other| *other != key && mob::is_mob(store, *other));
        if let Some(target) = target {
            mob::hurt(store, target, arrow.damage, None);
            remove(store, key);
            continue;
        }
        if let Some(data) = store.data_mut(key) {
            data.arrow = Some(arrow);
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mob::{MobKind, new_mob};
    use crate::physics::tests::TestWorld;

    fn spawn_arrow(store: &mut EntityStore, pos: Vec3, vel: Vec3) -> EntityKey {
        store.insert(new_arrow(pos, vel, 4.0, None, &mut Rng::new(1)))
    }

    #[test]
    fn arrows_fall_with_gravity_and_stick_in_blocks() {
        let world = TestWorld::flat(64);
        let mut store = EntityStore::default();
        // One step above the floor, falling: the next tick sticks in it.
        let key = spawn_arrow(
            &mut store,
            Vec3::new(0.5, 64.6, 0.5),
            Vec3::new(0.0, -1.0, 0.0),
        );
        tick_arrows(&mut store, &world, &[]);
        assert!(
            store.flags(key).unwrap().has(Flags::REMOVED),
            "an arrow flying into a floor sticks"
        );
    }

    #[test]
    fn an_arrow_hurts_a_mob_it_passes_through() {
        let world = TestWorld::flat(64);
        let mut store = EntityStore::default();
        let cow = store.insert(new_mob(
            MobKind::Cow,
            Vec3::new(0.5, 64.0, 6.0),
            &mut Rng::new(2),
        ));
        spawn_arrow(
            &mut store,
            Vec3::new(0.5, 64.9, 0.5),
            Vec3::new(0.0, 0.0, 2.0),
        );
        for _ in 0..40 {
            tick_arrows(&mut store, &world, &[]);
            if mob::state(&store, cow).is_none_or(|mob| mob.health < MobKind::Cow.max_health()) {
                break;
            }
        }
        assert!(
            mob::state(&store, cow).unwrap().health < MobKind::Cow.max_health(),
            "arrow hit the cow"
        );
    }

    #[test]
    fn arrows_report_player_hits_after_the_owner_grace() {
        let world = TestWorld::flat(64);
        let mut store = EntityStore::default();
        let uuid = Uuid::from_u128(7);
        // The box spans the arrow's whole flight, so the owner grace is what delays the hit.
        let box_ = AABB::new(0.2, 64.0, 0.2, 0.8, 70.0, 40.0);
        spawn_arrow(
            &mut store,
            Vec3::new(0.5, 68.0, 0.5),
            Vec3::new(0.0, 0.0, 0.2),
        );
        let mut hits = Vec::new();
        for _ in 0..40 {
            hits.extend(tick_arrows(&mut store, &world, &[(uuid, box_)]));
            if !hits.is_empty() {
                break;
            }
        }
        assert_eq!(hits, [PlayerHit { uuid, damage: 4.0 }]);
    }
}
