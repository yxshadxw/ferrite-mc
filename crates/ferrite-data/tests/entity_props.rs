//! Tests for the entity type properties extracted from the vanilla server by `tools/extractor`.

use ferrite_data::entity_props;

fn named(name: &str) -> &'static entity_props::EntityProps {
    entity_props::by_name(name).unwrap_or_else(|| panic!("{name} is not an entity type"))
}

#[test]
fn entity_type_count() {
    println!("{} entity types", entity_props::ENTITY_TYPE_COUNT);
    assert_eq!(entity_props::ENTITY_TYPE_COUNT, 161);
    for id in 0..entity_props::ENTITY_TYPE_COUNT as u16 {
        assert_eq!(entity_props::get(id).id, id);
    }
}

#[test]
fn player_dimensions() {
    let player = named("player");
    assert_eq!(player.key, "minecraft:player");
    assert_eq!((player.width, player.height), (0.6, 1.8));
    assert_eq!(player.eye_height, 1.62);
    assert_eq!(player.category, "misc");
    assert!(!player.is_summonable, "players cannot be summoned");
}

#[test]
fn creeper_is_a_monster() {
    assert_eq!(named("creeper").category, "monster");
    assert_eq!(named("minecraft:creeper").category, "monster");
}

#[test]
fn zombie_has_twenty_health() {
    let zombie = named("zombie");
    assert_eq!(
        entity_props::default_attribute(zombie.id, "max_health"),
        Some(20.0)
    );
    assert_eq!(
        entity_props::default_attribute(zombie.id, "minecraft:max_health"),
        Some(20.0)
    );
    assert_eq!(
        entity_props::default_attribute(zombie.id, "not_an_attribute"),
        None
    );
}

#[test]
fn cow_speed_is_point_two() {
    let cow = named("cow");
    let speed = entity_props::default_attribute(cow.id, "movement_speed")
        .expect("cows have a movement speed");
    assert!((speed - 0.2).abs() < 1e-6, "cow movement speed is {speed}");
    assert_eq!(
        entity_props::default_attribute(cow.id, "follow_range"),
        Some(16.0)
    );
}

#[test]
fn ender_dragon_is_summonable() {
    let dragon = named("ender_dragon");
    assert!(dragon.is_summonable, "vanilla summons the ender dragon");
    assert_eq!(dragon.category, "monster");
    assert!(dragon.fire_immune);
    assert_eq!(dragon.height, 8.0);
}

#[test]
fn item_frame_tracking_range() {
    let item_frame = named("item_frame");
    assert_eq!(item_frame.client_tracking_range, 10);
    assert_eq!(item_frame.width, 0.5);
    assert_eq!(item_frame.update_interval, i32::MAX);
    assert!(item_frame.attributes.is_empty());
}

#[test]
fn unknown_entity_types_are_none() {
    assert!(entity_props::by_name("not_an_entity").is_none());
}

#[test]
fn attachments_are_single_points() {
    let player = named("player");
    let passenger = player
        .attachments
        .iter()
        .find(|(name, _)| *name == "passenger")
        .expect("players have a passenger attachment");
    assert_eq!(passenger.1, [0.0, 1.8, 0.0]);

    let happy_ghast = named("happy_ghast");
    assert!(
        !happy_ghast
            .attachments
            .iter()
            .any(|(name, _)| *name == "passenger"),
        "the multi-point happy ghast seats are not simple attachments"
    );
}
