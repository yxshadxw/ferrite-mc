//! Experience orbs (vanilla `ExperienceOrb`): the death-drop split, lifetime, merge scan,
//! attraction to nearby players and pickup.
//!
//! The synched `DATA_VALUE` (index 8, an INT) drives the client orb size. `count` is the number
//! of orbs merged into one entity, exactly like vanilla: pickup awards the value once per count.

use crate::metadata::experience::VALUE;
use crate::physics::{self, BlockGetter};
use crate::rng::Rng;
use crate::store::{EntityData, EntityRecord, EntityStore, Flags};
use crate::tracker::{self, Sink};
use crate::{EntityKey, allocate_id, random_uuid};
use ferrite_math::{AABB, Vec3};
use ferrite_protocol::{
    VarInt,
    packets::play::{MetaValue, TakeItemEntity},
};
use std::sync::OnceLock;

/// `ExperienceOrb.LIFETIME` (five minutes).
pub const LIFETIME: i32 = 6000;
/// `ExperienceOrb` is a 0.5 x 0.5 entity in vanilla's entity report.
const WIDTH: f64 = 0.5;
const HEIGHT: f64 = 0.5;
/// `ExperienceOrb.getDefaultGravity`.
const GRAVITY: f64 = 0.03;
/// `ExperienceOrb.getAirDrag`; friction multiplies it on the ground.
const AIR_DRAG: f64 = 0.98;
/// `ExperienceOrb.MAX_FOLLOW_DIST`.
const FOLLOW_DIST: f64 = 8.0;

/// The fields of `ExperienceOrb` that are not synched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrbState {
    pub age: i32,
    /// Vanilla `count`: how many value-sized orbs were merged into this entity.
    pub count: i32,
}

impl Default for OrbState {
    fn default() -> Self {
        Self { age: 0, count: 1 }
    }
}

/// Entity type id of `minecraft:experience_orb`.
pub fn orb_type() -> u16 {
    static KIND: OnceLock<u16> = OnceLock::new();
    *KIND.get_or_init(|| {
        ferrite_data::entity_props::by_name("experience_orb")
            .expect("vanilla has an experience_orb entity type")
            .id
    })
}

/// The synched `DATA_VALUE` of an orb.
pub fn value(data: &EntityData) -> i32 {
    match data.metadata.get(VALUE) {
        Some(MetaValue::Int(value)) => *value,
        _ => 0,
    }
}

fn set_value(data: &mut EntityData, value: i32) {
    data.metadata.set(VALUE, MetaValue::Int(value));
}

/// A new orb (vanilla `new ExperienceOrb(level, x, y, z, value)`).
pub fn new_orb(pos: Vec3, value: i32) -> EntityRecord {
    let mut data = EntityData::new(allocate_id(), random_uuid(), orb_type(), WIDTH, HEIGHT);
    data.metadata.define(VALUE, MetaValue::Int(0));
    set_value(&mut data, value);
    data.orb = Some(OrbState::default());
    EntityRecord {
        pos,
        vel: Vec3::ZERO,
        flags: Flags::default(),
        data,
    }
}

/// `ExperienceOrb.getExperienceValue`: the largest orb vanilla splits an award into.
pub fn experience_value(max: i32) -> i32 {
    const STEPS: [(i32, i32); 10] = [
        (2477, 2477),
        (1237, 1237),
        (617, 617),
        (307, 307),
        (149, 149),
        (73, 73),
        (37, 37),
        (17, 17),
        (7, 7),
        (3, 3),
    ];
    for (threshold, value) in STEPS {
        if max >= threshold {
            return value;
        }
    }
    1
}

/// `ExperienceOrb.award`: splits `amount` into vanilla-sized orbs at `pos`.
///
/// Vanilla may merge each new piece into an existing orb within one block (a 1-in-40 scan);
/// here the pieces spawn as entities and the normal merge scan folds them together.
pub fn award(pos: Vec3, mut amount: i32) -> Vec<EntityRecord> {
    let mut orbs = Vec::new();
    while amount > 0 {
        let part = experience_value(amount);
        amount -= part;
        orbs.push(new_orb(pos, part));
    }
    orbs
}

fn nearest_player(players: &[Vec3], pos: Vec3) -> Option<Vec3> {
    let mut best: Option<(f64, Vec3)> = None;
    for &player in players {
        let distance = player.distance_to_sqr(pos);
        if distance <= FOLLOW_DIST * FOLLOW_DIST && best.is_none_or(|(best, _)| distance < best) {
            best = Some((distance, player));
        }
    }
    best.map(|(_, player)| player)
}

/// Ticks every experience orb in the store (vanilla `ExperienceOrb.tick`).
///
/// `players` are the positions of the region's alive players (spectators do not attract
/// orbs), used by `followNearbyPlayer`.
pub fn tick_orbs(
    store: &mut EntityStore,
    blocks: &impl BlockGetter,
    players: &[Vec3],
    rng: &mut Rng,
) {
    let kind = orb_type();
    let keys = store.keys().to_vec();
    for key in keys {
        let Some(data) = store.data(key) else {
            continue;
        };
        if data.kind != kind || data.orb.is_none() {
            continue;
        }
        if store
            .flags(key)
            .is_none_or(|flags| flags.has(Flags::REMOVED))
        {
            continue;
        }
        tick_orb(store, key, blocks, players, rng);
    }
}

fn tick_orb(
    store: &mut EntityStore,
    key: EntityKey,
    blocks: &impl BlockGetter,
    players: &[Vec3],
    rng: &mut Rng,
) {
    let Some(data) = store.data_mut(key) else {
        return;
    };
    // The level increments `tickCount` before ticking (`ServerLevel.tickNonPassenger`).
    data.tick_count += 1;
    let tick_count = data.tick_count;
    let Some(orb) = data.orb.as_mut() else {
        return;
    };
    orb.age += 1;
    if orb.age >= LIFETIME {
        if let Some(flags) = store.flags_mut(key) {
            flags.set(Flags::REMOVED, true);
        }
        return;
    }
    let old_pos = store.pos(key).unwrap();
    let bbox = store.bbox(key).unwrap();
    let blocked = !physics::no_collision(blocks, bbox.deflate_all(1.0E-7));
    let mut vel = store.vel(key).unwrap();
    if physics::fluid_height(blocks, bbox, false) > 0.1 {
        // `setUnderwaterMovement`: slow drag and a little buoyancy.
        vel = Vec3::new(
            vel.x * 0.99,
            vel.y + if vel.y < 0.06 { 5.0E-4 } else { 0.0 },
            vel.z * 0.99,
        );
    } else if !blocked {
        vel.y -= GRAVITY;
    }
    // `followNearbyPlayer`: accelerate toward the nearest player within eight blocks.
    if let Some(target) = nearest_player(players, old_pos) {
        // The player's eye height is 1.62; vanilla aims at `getEyeHeight() / 2`.
        let aim = Vec3::new(target.x, target.y + 0.81, target.z);
        let delta = aim - old_pos;
        let factor = 1.0 - delta.length() / FOLLOW_DIST;
        if factor > 0.0 {
            vel += delta.normalize() * (factor * factor * 0.1);
        }
    }
    store.set_vel(key, vel);
    if blocked {
        let center = Vec3::new(old_pos.x, (bbox.min.y + bbox.max.y) / 2.0, old_pos.z);
        physics::move_towards_closest_space(store, key, center, blocks, rng);
    }
    physics::move_entity(store, key, vel, blocks);
    let pos = store.pos(key).unwrap();
    let on_ground = store.flags(key).unwrap().has(Flags::ON_GROUND);
    let drag = if on_ground {
        AIR_DRAG * physics::friction(blocks, physics::on_pos(pos, 0.999_999))
    } else {
        AIR_DRAG
    };
    let mut vel = store.vel(key).unwrap();
    vel = Vec3::new(vel.x * drag, vel.y * AIR_DRAG, vel.z * drag);
    // Vanilla orbs lose 60% of their downward speed on landing instead of stopping.
    if on_ground && vel.y < 0.0 {
        vel.y *= -0.4;
    }
    store.set_vel(key, vel);
    if tick_count % 20 == 1 {
        merge_nearby(store, key);
    }
}

/// `ExperienceOrb.scanForMerges`: nearby orbs fold their `count` into this one.
fn merge_nearby(store: &mut EntityStore, key: EntityKey) {
    let Some(bbox) = store.bbox(key) else {
        return;
    };
    let kind = orb_type();
    for other in store.query_vec(bbox.inflate(0.5, 0.5, 0.5)) {
        if other == key {
            continue;
        }
        let (Some(this), Some(that)) = (store.data(key), store.data(other)) else {
            continue;
        };
        if that.kind != kind || that.orb.is_none() {
            continue;
        }
        if store
            .flags(other)
            .is_none_or(|flags| flags.has(Flags::REMOVED))
        {
            continue;
        }
        let (Some(this_orb), Some(that_orb)) = (this.orb, that.orb) else {
            continue;
        };
        if let Some(orb) = store.data_mut(key).and_then(|data| data.orb.as_mut()) {
            orb.count += that_orb.count;
            orb.age = this_orb.age.min(that_orb.age);
        }
        if let Some(flags) = store.flags_mut(other) {
            flags.set(Flags::REMOVED, true);
        }
    }
}

/// `Player.touch` + `ExperienceOrb.playerTouch`: awards every orb intersecting the player's
/// box widened by 1 x 0.5 x 1 and broadcasts `take_item_entity` to its viewers.
pub fn player_touch(
    store: &mut EntityStore,
    entity_id: i32,
    feet: Vec3,
    sink: &mut Sink<'_>,
) -> i32 {
    let area = AABB::new(
        feet.x - 0.3,
        feet.y,
        feet.z - 0.3,
        feet.x + 0.3,
        feet.y + 1.8,
        feet.z + 0.3,
    )
    .inflate(1.0, 0.5, 1.0);
    let kind = orb_type();
    let mut gained = 0;
    for key in store.query_vec(area) {
        let (Some(data), Some(flags)) = (store.data(key), store.flags(key)) else {
            continue;
        };
        if data.kind != kind || flags.has(Flags::REMOVED) {
            continue;
        }
        let Some(orb) = data.orb else {
            continue;
        };
        gained += value(data);
        tracker::broadcast(
            store,
            key,
            &TakeItemEntity {
                item: VarInt(data.id),
                collector: VarInt(entity_id),
                amount: VarInt(1),
            },
            sink,
        );
        if orb.count <= 1 {
            if let Some(flags) = store.flags_mut(key) {
                flags.set(Flags::REMOVED, true);
            }
        } else if let Some(orb) = store.data_mut(key).and_then(|data| data.orb.as_mut()) {
            orb.count -= 1;
        }
    }
    gained
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::tests::TestWorld;

    #[test]
    fn award_splits_into_vanilla_sizes() {
        assert_eq!(experience_value(1), 1);
        assert_eq!(experience_value(2), 1);
        assert_eq!(experience_value(3), 3);
        assert_eq!(experience_value(7), 7);
        assert_eq!(experience_value(16), 7);
        assert_eq!(experience_value(100), 73);
        let orbs = award(Vec3::new(0.5, 64.0, 0.5), 100);
        let values: Vec<_> = orbs.iter().map(|orb| value(&orb.data)).collect();
        assert_eq!(values, [73, 17, 7, 3]);
        assert!(orbs.iter().all(|orb| orb.data.kind == orb_type()));
        assert!(
            orbs.iter()
                .all(|orb| orb.data.orb == Some(OrbState::default()))
        );
        assert!(award(Vec3::ZERO, 0).is_empty());
    }

    #[test]
    fn nearby_orbs_merge_counts_and_keep_the_oldest_age() {
        let world = TestWorld::flat(64);
        let mut store = EntityStore::default();
        let mut rng = Rng::new(5);
        let a = store.insert(new_orb(Vec3::new(0.5, 64.0, 0.5), 7));
        let b = store.insert(new_orb(Vec3::new(0.9, 64.0, 0.5), 7));
        store.data_mut(b).unwrap().orb.as_mut().unwrap().age = 9;
        for _ in 0..21 {
            tick_orbs(&mut store, &world, &[], &mut rng);
        }
        // The first orb absorbs the second on the `tickCount % 20 == 1` scan.
        assert!(!store.flags(a).unwrap().has(Flags::REMOVED));
        assert!(store.flags(b).unwrap().has(Flags::REMOVED));
        let merged = store.data(a).unwrap().orb.unwrap();
        assert_eq!(merged.count, 2);
        assert_eq!(merged.age, 21, "the merged orb keeps the youngest age");
    }

    #[test]
    fn pickup_awards_the_value_and_sends_take_item_entity() {
        use std::collections::HashSet;
        let mut store = EntityStore::default();
        let key = store.insert(new_orb(Vec3::new(0.5, 64.0, 0.5), 7));
        let chunks = HashSet::from([(0, 0)]);
        let viewer = tracker::Viewer {
            key: (uuid::Uuid::from_u128(1), 0),
            pos: Vec3::new(0.5, 64.0, 0.5),
            view_distance: 2,
            chunks: &chunks,
        };
        let mut sent = Vec::new();
        {
            let mut sink = |_: tracker::ViewerKey, packet: &[u8]| sent.push(packet.to_vec());
            tracker::track(&mut store, &[viewer], &mut sink);
        }
        // Drop the AddEntity/SetEntityData packets; only the take packet is under test.
        sent.clear();
        {
            let mut sink = |_: tracker::ViewerKey, packet: &[u8]| sent.push(packet.to_vec());
            assert_eq!(
                player_touch(&mut store, 42, Vec3::new(0.5, 64.0, 0.5), &mut sink),
                7
            );
        }
        assert!(store.flags(key).unwrap().has(Flags::REMOVED));
        assert_eq!(sent.len(), 1);
        // Out of reach: no second award.
        let mut sink = |_: tracker::ViewerKey, _: &[u8]| {};
        assert_eq!(
            player_touch(&mut store, 42, Vec3::new(10.0, 64.0, 0.5), &mut sink),
            0
        );
    }

    #[test]
    fn orbs_expire_after_their_lifetime() {
        let world = TestWorld::flat(64);
        let mut store = EntityStore::default();
        let mut rng = Rng::new(1);
        let key = store.insert(new_orb(Vec3::new(0.5, 64.0, 0.5), 3));
        store.data_mut(key).unwrap().orb.as_mut().unwrap().age = LIFETIME - 2;
        tick_orbs(&mut store, &world, &[], &mut rng);
        assert!(!store.flags(key).unwrap().has(Flags::REMOVED));
        tick_orbs(&mut store, &world, &[], &mut rng);
        assert!(store.flags(key).unwrap().has(Flags::REMOVED));
    }
}
