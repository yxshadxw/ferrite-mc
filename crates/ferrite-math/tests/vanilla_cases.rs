use ferrite_math::{
    AABB, BlockPos, ChunkPos, Direction, SectionPos, Vec3, VoxelShape, collide,
    look_from_yaw_pitch, mth, raycast, yaw_pitch_from_look,
};

fn assert_vec_near(actual: Vec3, expected: Vec3) {
    assert!(
        (actual - expected).length() < 1e-9,
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn walking_into_wall_stops_at_face() {
    let player = AABB::new(0.0, 0.0, 0.0, 0.6, 1.8, 0.6);
    let wall = AABB::new(1.0, 0.0, 0.0, 2.0, 2.0, 1.0);

    let moved = collide(player, Vec3::new(0.5, 0.0, 0.0), &[wall]);

    assert!((moved.x - 0.4).abs() < 1e-9, "moved {moved:?}");
    assert_eq!(moved.y, 0.0);
    assert_eq!(moved.z, 0.0);
    assert!((player.move_by(moved).max.x - 1.0).abs() < 1e-9);
}

#[test]
fn step_up_onto_slab_works() {
    let player = AABB::new(0.0, 0.0, 0.0, 1.0, 1.8, 1.0);
    let slab = AABB::new(1.0, 0.0, 0.0, 2.0, 0.5, 1.0);

    let moved = collide(player, Vec3::new(0.5, 0.0, 0.0), &[slab]);

    assert!((moved.x - 0.5).abs() < 1e-9, "moved {moved:?}");
    assert!((moved.y - 0.5).abs() < 1e-9, "moved {moved:?}");
    assert_eq!(moved.z, 0.0);

    let after = player.move_by(moved);
    assert!((after.min.y - 0.5).abs() < 1e-9);
    assert!((after.max.x - 1.5).abs() < 1e-9);
}

#[test]
fn stepping_onto_full_block_does_not_work() {
    let player = AABB::new(0.0, 0.0, 0.0, 1.0, 1.8, 1.0);
    let wall = AABB::new(1.0, 0.0, 0.0, 2.0, 1.0, 1.0);

    let moved = collide(player, Vec3::new(0.5, 0.0, 0.0), &[wall]);

    assert!(moved.x.abs() < 1e-9, "moved {moved:?}");
    assert!(moved.y.abs() < 1e-9, "moved {moved:?}");
    assert!(moved.z.abs() < 1e-9, "moved {moved:?}");
}

#[test]
fn falling_onto_ground_stops_at_y() {
    let player = AABB::new(0.0, 2.0, 0.0, 0.6, 3.8, 0.6);
    let ground = AABB::new(-4.0, 0.0, -4.0, 4.0, 1.0, 4.0);

    let moved = collide(player, Vec3::new(0.0, -2.0, 0.0), &[ground]);
    assert!((moved.y + 1.0).abs() < 1e-9, "moved {moved:?}");
    assert!((player.move_by(moved).min.y - 1.0).abs() < 1e-9);

    let standing = AABB::new(0.0, 1.0, 0.0, 0.6, 2.8, 0.6);
    let settled = collide(standing, Vec3::new(0.0, -0.1, 0.0), &[ground]);
    assert_eq!(settled.y, 0.0);
}

#[test]
fn sub_epsilon_gap_is_treated_as_touching() {
    let player = AABB::new(0.0, 0.0, 0.0, 1.0 - 1e-8, 1.8, 1.0);
    let wall = AABB::new(1.0, 0.0, 0.0, 2.0, 2.0, 1.0);

    let moved = collide(player, Vec3::new(0.5, 0.0, 0.0), &[wall]);

    assert_eq!(moved.x, 0.0);
    assert_eq!(moved.y, 0.0);
}

#[test]
fn block_raycast_hits_correct_face() {
    let shape = VoxelShape::full_block();
    let target = BlockPos::new(1, 0, 0);
    let hit = raycast(
        Vec3::new(0.5, 0.5, 0.5),
        Vec3::new(1.0, 0.0, 0.0),
        5.0,
        |pos| {
            if pos == target {
                Some(shape.boxes())
            } else {
                None
            }
        },
    )
    .expect("ray should hit the block");

    assert_eq!(hit.pos, target);
    assert_eq!(hit.face, Direction::West);
    assert_vec_near(hit.location, Vec3::new(1.0, 0.5, 0.5));

    let from_east = raycast(
        Vec3::new(2.5, 0.5, 0.5),
        Vec3::new(-1.0, 0.0, 0.0),
        5.0,
        |pos| {
            if pos == target {
                Some(shape.boxes())
            } else {
                None
            }
        },
    )
    .expect("ray should hit the block");

    assert_eq!(from_east.face, Direction::East);
    assert_vec_near(from_east.location, Vec3::new(2.0, 0.5, 0.5));
}

#[test]
fn block_raycast_hits_top_face_from_above() {
    let shape = VoxelShape::full_block();
    let target = BlockPos::new(0, 0, 0);
    let hit = raycast(
        Vec3::new(0.5, 2.5, 0.5),
        Vec3::new(0.0, -1.0, 0.0),
        5.0,
        |pos| {
            if pos == target {
                Some(shape.boxes())
            } else {
                None
            }
        },
    )
    .expect("ray should hit the block");

    assert_eq!(hit.pos, target);
    assert_eq!(hit.face, Direction::Up);
    assert_vec_near(hit.location, Vec3::new(0.5, 1.0, 0.5));
}

#[test]
fn block_raycast_through_gap_misses() {
    let shape = VoxelShape::full_block();
    let left = BlockPos::new(0, 0, 0);
    let right = BlockPos::new(2, 0, 0);

    let hit = raycast(
        Vec3::new(1.5, 0.5, 2.5),
        Vec3::new(0.0, 0.0, -1.0),
        5.0,
        |pos| {
            if pos == left || pos == right {
                Some(shape.boxes())
            } else {
                None
            }
        },
    );

    assert!(hit.is_none());
}

#[test]
fn negative_coordinates_floor_for_sections_and_chunks() {
    assert_eq!(
        BlockPos::containing(Vec3::new(-0.1, -1.0, -16.1)),
        BlockPos::new(-1, -1, -17)
    );
    assert_eq!(BlockPos::new(-1, 64, -1).chunk_pos(), ChunkPos::new(-1, -1));
    assert_eq!(
        BlockPos::new(-17, 64, -1).section_pos(),
        SectionPos::new(-2, 4, -1)
    );
    assert_eq!(mth::floor(-0.5), -1);
}

#[test]
fn yaw_pitch_and_look_vectors_round_trip() {
    let south = look_from_yaw_pitch(0.0, 0.0);
    assert!(south.z > 0.999 && south.x.abs() < 1e-6);

    let west = look_from_yaw_pitch(90.0, 0.0);
    assert!(west.x < -0.999);

    let (yaw, pitch) = yaw_pitch_from_look(look_from_yaw_pitch(-45.0, 30.0));
    assert!((yaw + 45.0).abs() < 1e-4, "yaw was {yaw}");
    assert!((pitch - 30.0).abs() < 1e-4, "pitch was {pitch}");
}
