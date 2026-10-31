//! A* pathfinding on the block grid, with the vanilla `WalkNodeEvaluator` subset the modelled
//! mobs need: stand on a solid block with two passable blocks above, walk straight or
//! diagonally, step up one block and drop up to three. Liquids are not walkable; unloaded
//! chunks are walls.
//!
//! Paths are small (at most [`MAX_PATH`] waypoints, [`MAX_NODES`] expanded nodes) because they
//! are computed on the region thread for a handful of mobs at a time, like vanilla's
//! `PathNavigation` region-limited search.

use crate::physics::BlockGetter;
use ferrite_data::{block_props, blocks};
use std::collections::{BinaryHeap, HashMap, HashSet};

/// Waypoints a single path keeps.
pub const MAX_PATH: usize = 16;
/// Expanded nodes before the search gives up.
pub const MAX_NODES: usize = 512;
/// Vanilla walkable node evaluator drops at most this far.
const MAX_DROP: i32 = 3;
/// Cost of a straight step, in tenths of a block.
const STRAIGHT: u32 = 10;
/// Cost of a diagonal step.
const DIAGONAL: u32 = 14;
/// Extra cost for a one-block step up or down.
const VERTICAL: u32 = 4;

fn state(blocks: &impl BlockGetter, x: i32, y: i32, z: i32) -> Option<u16> {
    blocks
        .block_state(x, y, z)
        .filter(|state| usize::from(*state) < blocks::STATE_COUNT)
}

/// A block a mob can stand on: solid collision, not a fluid.
fn walkable_ground(state: u16) -> bool {
    let props = block_props::get(state);
    !props.is_air && props.fluid.is_none() && !block_props::collision_shape(state).is_empty()
}

/// A block a mob can occupy: air or a shape without collision.
fn passable(state: u16) -> bool {
    let props = block_props::get(state);
    props.is_air || props.fluid.is_none() && block_props::collision_shape(state).is_empty()
}

/// Whether a mob can stand with its feet at `(x, y, z)`.
fn node_ok(blocks: &impl BlockGetter, x: i32, y: i32, z: i32) -> bool {
    let (Some(below), Some(feet), Some(head)) = (
        state(blocks, x, y - 1, z),
        state(blocks, x, y, z),
        state(blocks, x, y + 1, z),
    ) else {
        return false;
    };
    walkable_ground(below) && passable(feet) && passable(head)
}

/// The node a search starts from: the mob's own block, or the closest walkable level around it.
pub fn start_node(blocks: &impl BlockGetter, at: [i32; 3]) -> Option<[i32; 3]> {
    for dy in 0..=2 {
        let candidate = [at[0], at[1] + dy, at[2]];
        if node_ok(blocks, candidate[0], candidate[1], candidate[2]) {
            return Some(candidate);
        }
    }
    for dy in 1..=MAX_DROP {
        let candidate = [at[0], at[1] - dy, at[2]];
        if node_ok(blocks, candidate[0], candidate[1], candidate[2]) {
            return Some(candidate);
        }
    }
    None
}

/// A* from the block at `start` to the block at `goal`, as waypoint blocks (without the start
/// node), at most [`MAX_PATH`] long. An empty path means "no path found" or "already there".
pub fn find_path(
    blocks: &impl BlockGetter,
    start: [i32; 3],
    goal: [i32; 3],
    max_range: i32,
) -> Vec<[i32; 3]> {
    let Some(start) = start_node(blocks, start) else {
        return Vec::new();
    };
    let dx = goal[0] - start[0];
    let dz = goal[2] - start[2];
    if dx * dx + dz * dz > max_range * max_range {
        return Vec::new();
    }
    let heuristic = |p: [i32; 3]| -> u32 {
        let dx = (p[0] - goal[0]).unsigned_abs();
        let dz = (p[2] - goal[2]).unsigned_abs();
        let dy = (p[1] - goal[1]).unsigned_abs();
        (dx.max(dz) * DIAGONAL + dx.abs_diff(dz) * STRAIGHT + dy * VERTICAL) / 2
    };
    let at_goal = |p: [i32; 3]| {
        (p[0] - goal[0]).abs() <= 1 && (p[2] - goal[2]).abs() <= 1 && (p[1] - goal[1]).abs() <= 1
    };
    let mut open: BinaryHeap<(std::cmp::Reverse<u32>, [i32; 3])> = BinaryHeap::new();
    let mut cost: HashMap<[i32; 3], u32> = HashMap::new();
    let mut parent: HashMap<[i32; 3], [i32; 3]> = HashMap::new();
    let mut closed: HashSet<[i32; 3]> = HashSet::new();
    cost.insert(start, 0);
    open.push((std::cmp::Reverse(heuristic(start)), start));
    let mut goal_node = None;
    let mut expanded = 0;
    while let Some((_, node)) = open.pop() {
        if at_goal(node) {
            goal_node = Some(node);
            break;
        }
        if !closed.insert(node) {
            continue;
        }
        expanded += 1;
        if expanded > MAX_NODES {
            break;
        }
        let base = cost[&node];
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            // A diagonal needs both of its straight neighbours open, so mobs do not cut
            // corners through walls.
            if dx != 0
                && dz != 0
                && (!node_ok(blocks, node[0] + dx, node[1], node[2])
                    || !node_ok(blocks, node[0], node[1], node[2] + dz))
            {
                continue;
            }
            let x = node[0] + dx;
            let z = node[2] + dz;
            // Level ground first, then a step up, then drops up to three blocks.
            let mut next = None;
            for dy in [0, 1, -1, -2, -3] {
                if dy < 0 && dy < -MAX_DROP {
                    continue;
                }
                let y = node[1] + dy;
                if node_ok(blocks, x, y, z) {
                    // Standing at a height needs open space above the previous level too.
                    if dy > 0
                        && !passable(state(blocks, node[0], node[1] + 2, node[2]).unwrap_or(0))
                    {
                        continue;
                    }
                    next = Some([x, y, z]);
                    break;
                }
            }
            let Some(next) = next else {
                continue;
            };
            if closed.contains(&next) {
                continue;
            }
            let step = if dx != 0 && dz != 0 {
                DIAGONAL
            } else {
                STRAIGHT
            };
            let step = step + u32::try_from((next[1] - node[1]).abs()).unwrap_or(0) * VERTICAL;
            let tentative = base + step;
            if tentative < *cost.get(&next).unwrap_or(&u32::MAX) {
                cost.insert(next, tentative);
                parent.insert(next, node);
                open.push((std::cmp::Reverse(tentative + heuristic(next)), next));
            }
        }
    }
    let Some(mut node) = goal_node else {
        return Vec::new();
    };
    let mut path = Vec::new();
    while node != start && path.len() < MAX_PATH {
        path.push(node);
        let Some(&previous) = parent.get(&node) else {
            return Vec::new();
        };
        node = previous;
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A flat floor with optional walls.
    struct World {
        floor: i32,
        blocks: HashMap<[i32; 3], &'static str>,
    }

    impl World {
        fn flat(floor: i32) -> Self {
            Self {
                floor,
                blocks: HashMap::new(),
            }
        }

        fn wall(&mut self, x: i32, z: i32, from: i32, height: i32) {
            for y in from..from + height {
                self.blocks.insert([x, y, z], "stone");
            }
        }
    }

    impl BlockGetter for World {
        fn block_state(&self, x: i32, y: i32, z: i32) -> Option<u16> {
            if let Some(name) = self.blocks.get(&[x, y, z]) {
                return blocks::default_state(name);
            }
            blocks::default_state(if y < self.floor { "stone" } else { "air" })
        }
    }

    #[test]
    fn a_flat_path_is_straight_and_bounded() {
        let world = World::flat(64);
        let path = find_path(&world, [0, 64, 0], [5, 64, 0], 32);
        assert!(!path.is_empty());
        let last = *path.last().unwrap();
        // The search stops at the first node within one block of the goal.
        assert!(
            (last[0] - 5).abs() <= 1 && last[2].abs() <= 1,
            "{last:?}"
        );
        assert!(path.len() <= MAX_PATH);
        assert!(find_path(&world, [0, 64, 0], [5, 64, 0], 2).is_empty());
        assert!(find_path(&world, [0, 64, 0], [0, 64, 0], 8).is_empty());
    }

    #[test]
    fn a_wall_forces_a_detour() {
        let mut world = World::flat(64);
        for z in -6..=6 {
            world.wall(3, z, 64, 3);
        }
        let path = find_path(&world, [0, 64, 0], [6, 64, 0], 32);
        assert!(!path.is_empty(), "a detour exists around the wall");
        // No waypoint is inside the wall (which covers z -6..=6).
        assert!(path.iter().all(|p| p[0] != 3 || p[2].abs() > 6));
        let last = *path.last().unwrap();
        assert!((last[0] - 6).abs() <= 1, "{last:?}");
    }

    #[test]
    fn a_step_up_and_a_drop_are_walkable_but_a_wall_is_not() {
        let mut world = World::flat(64);
        for z in -2..=2 {
            for x in 2..=4 {
                world.blocks.insert([x, 64, z], "stone");
            }
        }
        let path = find_path(&world, [0, 64, 0], [6, 64, 0], 32);
        assert!(!path.is_empty(), "a one-block step is climbable");
        assert!(path.iter().any(|p| p[1] == 65));
        // A five-block wall has no waypoint inside it; the only route goes around the ends.
        let mut tall = World::flat(64);
        for z in -8..=8 {
            tall.wall(3, z, 64, 5);
        }
        let detour = find_path(&tall, [0, 64, 0], [6, 64, 0], 32);
        // The wall covers z -8..=8; x = 3 only appears beyond its ends.
        assert!(detour.iter().all(|p| p[0] != 3 || p[2].abs() > 8));
    }

    #[test]
    fn unloaded_columns_are_walls() {
        struct Void;
        impl BlockGetter for Void {
            fn block_state(&self, _x: i32, _y: i32, _z: i32) -> Option<u16> {
                None
            }
        }
        assert!(find_path(&Void, [0, 64, 0], [2, 64, 0], 32).is_empty());
    }
}
