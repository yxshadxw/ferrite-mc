use crate::EPSILON;
use crate::aabb::AABB;
use crate::direction::Axis;
use crate::pos::BlockPos;
use crate::vec3::Vec3;

/// Players can step up at most 0.6 blocks, like vanilla's `maxUpStep`.
pub const PLAYER_MAX_UP_STEP: f64 = 0.6;

/// A block collision shape, stored as boxes in block-local coordinates.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VoxelShape {
    boxes: Vec<AABB>,
}

impl VoxelShape {
    pub fn new(boxes: Vec<AABB>) -> Self {
        Self { boxes }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn full_block() -> Self {
        Self::new(vec![AABB::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)])
    }

    pub fn boxes(&self) -> &[AABB] {
        &self.boxes
    }

    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// The shape's boxes translated into world space at `pos`.
    pub fn world_boxes(&self, pos: BlockPos) -> impl Iterator<Item = AABB> + '_ {
        let offset = Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        self.boxes.iter().map(move |shape| shape.move_by(offset))
    }
}

/// Moves `entity_box` by `movement`, resolving collisions against world-space
/// boxes and allowing players to step up to [`PLAYER_MAX_UP_STEP`].
///
/// The result is the offset actually applied: Y is resolved first, then the
/// larger of X and Z, with the usual `1e-7` epsilon. Step-up is attempted
/// whenever horizontal movement is obstructed; pass `0.0` to
/// [`collide_with_step`] for entities that cannot step.
pub fn collide(entity_box: AABB, movement: Vec3, shapes: &[AABB]) -> Vec3 {
    collide_with_step(entity_box, movement, shapes, PLAYER_MAX_UP_STEP)
}

/// Like [`collide`], with a configurable maximum step-up height.
pub fn collide_with_step(
    entity_box: AABB,
    movement: Vec3,
    shapes: &[AABB],
    max_up_step: f64,
) -> Vec3 {
    if movement.length_sqr() < EPSILON * EPSILON {
        return Vec3::ZERO;
    }

    let dy = collide_axis(entity_box, Axis::Y, movement.y, shapes);
    let after_y = entity_box.move_by(Vec3::new(0.0, dy, 0.0));

    let (dx, dz) = collide_horizontal(after_y, movement, shapes);
    let mut result = Vec3::new(dx, dy, dz);

    if max_up_step > 0.0 {
        let blocked_x = movement.x.abs() > EPSILON && (movement.x - dx).abs() > EPSILON;
        let blocked_z = movement.z.abs() > EPSILON && (movement.z - dz).abs() > EPSILON;

        if blocked_x || blocked_z {
            let up = collide_axis(after_y, Axis::Y, max_up_step, shapes);
            if up > EPSILON {
                let raised = after_y.move_by(Vec3::new(0.0, up, 0.0));
                let (step_x, step_z) = collide_horizontal(raised, movement, shapes);
                let gained = horizontal_gain(movement.x, dx, step_x)
                    + horizontal_gain(movement.z, dz, step_z);

                if gained > EPSILON {
                    let stepped = raised.move_by(Vec3::new(step_x, 0.0, step_z));
                    let down = collide_axis(stepped, Axis::Y, -up, shapes);
                    let stepped_y = dy + up + down;
                    if stepped_y > dy + EPSILON {
                        result = Vec3::new(step_x, stepped_y, step_z);
                    }
                }
            }
        }
    }

    result
}

fn horizontal_gain(requested: f64, before: f64, after: f64) -> f64 {
    if requested.abs() < EPSILON {
        0.0
    } else {
        (after - before) * requested.signum()
    }
}

/// Resolves both horizontal axes, largest movement first, after the vertical
/// pass.
fn collide_horizontal(entity_box: AABB, movement: Vec3, shapes: &[AABB]) -> (f64, f64) {
    if movement.x.abs() >= movement.z.abs() {
        let dx = collide_axis(entity_box, Axis::X, movement.x, shapes);
        let after_x = entity_box.move_by(Vec3::new(dx, 0.0, 0.0));
        let dz = collide_axis(after_x, Axis::Z, movement.z, shapes);
        (dx, dz)
    } else {
        let dz = collide_axis(entity_box, Axis::Z, movement.z, shapes);
        let after_z = entity_box.move_by(Vec3::new(0.0, 0.0, dz));
        let dx = collide_axis(after_z, Axis::X, movement.x, shapes);
        (dx, dz)
    }
}

/// Sweeps `box` along one axis against `shapes` and returns the allowed
/// movement, clamped to the requested one.
fn collide_axis(entity_box: AABB, axis: Axis, movement: f64, shapes: &[AABB]) -> f64 {
    if movement.abs() < EPSILON {
        return 0.0;
    }

    let min = entity_box.min.component(axis);
    let max = entity_box.max.component(axis);
    let mut closest = movement;

    for shape in shapes {
        if !overlaps_on_other_axes(entity_box, *shape, axis) {
            continue;
        }

        let shape_min = shape.min.component(axis);
        let shape_max = shape.max.component(axis);

        if movement > 0.0 {
            let gap = shape_min - max;
            if gap < -EPSILON {
                // The box already overlaps this shape on the moving axis.
                closest = 0.0;
            } else if gap < closest {
                closest = gap.max(0.0);
            }
        } else {
            let gap = shape_max - min;
            if gap > EPSILON {
                closest = 0.0;
            } else if gap > closest {
                closest = gap.min(0.0);
            }
        }
    }

    if closest.abs() < EPSILON {
        0.0
    } else {
        closest
    }
}

fn overlaps_on_other_axes(a: AABB, b: AABB, axis: Axis) -> bool {
    Axis::ALL.iter().all(|&other| {
        other == axis
            || (a.min.component(other) < b.max.component(other)
                && a.max.component(other) > b.min.component(other))
    })
}

#[cfg(test)]
mod tests {
    use super::{VoxelShape, collide, collide_with_step};
    use crate::aabb::AABB;
    use crate::pos::BlockPos;
    use crate::vec3::Vec3;

    #[test]
    fn world_boxes_are_offset_by_block_position() {
        let shape = VoxelShape::full_block();
        let world = shape.world_boxes(BlockPos::new(-2, 5, 7)).next().unwrap();
        assert_eq!(world, AABB::new(-2.0, 5.0, 7.0, -1.0, 6.0, 8.0));
        assert!(VoxelShape::empty().is_empty());
    }

    #[test]
    fn step_up_can_be_disabled() {
        let player = AABB::new(0.0, 0.0, 0.0, 1.0, 1.8, 1.0);
        let slab = AABB::new(1.0, 0.0, 0.0, 2.0, 0.5, 1.0);

        let stepped = collide(player, Vec3::new(0.5, 0.0, 0.0), &[slab]);
        assert!((stepped.y - 0.5).abs() < 1e-9);

        let flat = collide_with_step(player, Vec3::new(0.5, 0.0, 0.0), &[slab], 0.0);
        assert_eq!(flat, Vec3::ZERO);
    }
}
