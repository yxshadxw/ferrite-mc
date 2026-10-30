//! Scheduled fluid simulation, based on `FlowingFluid.java#tick`, `#spread`,
//! `#getSpread` and `#getSlopeDistance`. Partial collision faces use the
//! approximation documented by `can_pass_through`.
use crate::{
    FluidKind, FluidState, FluidWorld, block_for, can_hold_fluid, can_pass_through, fluid_state,
    get_new_liquid, tick_delay,
};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

pub type Position = (i32, i32, i32);
const SIDES: [(i32, i32, i32); 4] = [(0, 0, -1), (0, 0, 1), (-1, 0, 0), (1, 0, 0)];
const NEIGHBORS: [(i32, i32, i32); 6] = [
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
    (0, -1, 0),
    (0, 1, 0),
];
fn offset(p: Position, d: Position) -> Option<Position> {
    Some((
        p.0.checked_add(d.0)?,
        p.1.checked_add(d.1)?,
        p.2.checked_add(d.2)?,
    ))
}
fn block(w: &impl FluidWorld, p: Position) -> u32 {
    w.block(p.0, p.1, p.2)
}

/// A tick-ordered queue; duplicate positions retain the first due tick.
#[derive(Default)]
pub struct FluidTicks {
    now: u64,
    seq: u64,
    queue: BinaryHeap<Reverse<(u64, u64, Position)>>,
    pending: HashMap<Position, (u64, u64)>,
}
impl FluidTicks {
    pub fn schedule(&mut self, pos: Position, delay: u64) {
        let due = self.now.saturating_add(delay);
        if self.pending.contains_key(&pos) {
            return;
        }
        let seq = self.seq;
        self.seq += 1;
        self.pending.insert(pos, (due, seq));
        self.queue.push(Reverse((due, seq, pos)));
    }
    /// Processes due work in due-time then insertion order; later work remains queued.
    pub fn run_until(&mut self, world: &mut impl FluidWorld, tick: u64) {
        assert!(tick >= self.now, "cannot rewind fluid ticks");
        while let Some(&Reverse((due, seq, pos))) = self.queue.peek() {
            if due > tick {
                break;
            }
            self.queue.pop();
            if self.pending.get(&pos) != Some(&(due, seq)) {
                continue;
            }
            self.pending.remove(&pos);
            self.now = due;
            self.tick(world, pos);
        }
        self.now = tick;
    }
    fn changed(&mut self, world: &impl FluidWorld, pos: Position, kind: FluidKind) {
        let delay = u64::from(tick_delay(kind, world.ultrawarm()));
        if delay > 0 {
            self.schedule(pos, delay);
        }
        for d in NEIGHBORS {
            if let Some(n) = offset(pos, d) {
                let f = fluid_state(block(world, n));
                if f.kind != FluidKind::Empty {
                    self.schedule(n, u64::from(tick_delay(f.kind, world.ultrawarm())));
                }
            }
        }
    }
    fn put(&mut self, world: &mut impl FluidWorld, pos: Position, state: FluidState) {
        let old = block(world, pos);
        // Preserve waterloggable blocks instead of replacing them with water.
        let new = if state.kind == FluidKind::Water && fluid_state(old).kind == FluidKind::Empty {
            u16::try_from(old)
                .ok()
                .and_then(ferrite_data::blocks::state)
                .and_then(|(name, properties)| {
                    if !properties
                        .iter()
                        .any(|&(k, v)| k == "waterlogged" && v == "false")
                    {
                        return None;
                    }
                    let properties: Vec<_> = properties
                        .iter()
                        .map(|&(k, v)| (k, if k == "waterlogged" { "true" } else { v }))
                        .collect();
                    ferrite_data::blocks::state_id(name, &properties).map(u32::from)
                })
                .unwrap_or_else(|| block_for(state))
        } else {
            block_for(state)
        };
        if old != new {
            world.set_block(pos.0, pos.1, pos.2, new);
            self.changed(world, pos, state.kind);
        }
    }
    fn tick(&mut self, world: &mut impl FluidWorld, pos: Position) {
        let mut current = fluid_state(block(world, pos));
        if current.kind == FluidKind::Empty {
            return;
        }
        if !current.source {
            let next = get_new_liquid(world, pos.0, pos.1, pos.2, current.kind, false);
            if next != current {
                self.put(world, pos, next);
                current = next;
            }
        }
        if current.kind == FluidKind::Empty {
            return;
        }
        let Some(down) = offset(pos, (0, -1, 0)) else {
            return;
        };
        let mut spread_down = false;
        if self.can_enter(world, pos, down, current.kind) {
            let next = get_new_liquid(world, down.0, down.1, down.2, current.kind, false);
            if next.kind != FluidKind::Empty {
                self.put(world, down, next);
                spread_down = true;
            }
        }
        if spread_down {
            let sources = SIDES
                .iter()
                .filter_map(|&d| offset(pos, d))
                .filter(|&n| {
                    let f = fluid_state(block(world, n));
                    f.kind == current.kind && f.source
                })
                .count();
            if sources < 3 {
                return;
            }
        } else if !current.source && self.hole(world, pos, current.kind) {
            return;
        }
        let drop = if current.kind == FluidKind::Lava && !world.ultrawarm() {
            2
        } else {
            1
        };
        if !current.falling && current.amount <= drop {
            return;
        }
        let limit = if current.kind == FluidKind::Water || world.ultrawarm() {
            4
        } else {
            2
        };
        let mut choices = Vec::new();
        let mut best = u32::MAX;
        for d in SIDES {
            let Some(n) = offset(pos, d) else { continue };
            if !self.can_explore(world, pos, n, current.kind) {
                continue;
            }
            let score = if self.hole(world, n, current.kind) {
                0
            } else {
                self.slope(world, n, pos, current.kind, 1, limit)
            };
            if score < best {
                choices.clear();
                best = score;
            }
            if score == best && self.can_enter(world, pos, n, current.kind) {
                choices.push(n);
            }
        }
        for n in choices {
            let next = get_new_liquid(world, n.0, n.1, n.2, current.kind, false);
            if next.kind != FluidKind::Empty {
                self.put(world, n, next);
            }
        }
    }
    fn can_enter(
        &self,
        w: &impl FluidWorld,
        from: Position,
        to: Position,
        kind: FluidKind,
    ) -> bool {
        let target = block(w, to);
        fluid_state(target).kind == FluidKind::Empty
            && can_hold_fluid(target, kind)
            && can_pass_through(block(w, from), target)
    }
    fn can_explore(
        &self,
        w: &impl FluidWorld,
        from: Position,
        to: Position,
        kind: FluidKind,
    ) -> bool {
        let target = block(w, to);
        let fluid = fluid_state(target);
        (fluid.kind == FluidKind::Empty || (fluid.kind == kind && !fluid.source))
            && can_hold_fluid(target, kind)
            && can_pass_through(block(w, from), target)
    }
    fn hole(&self, w: &impl FluidWorld, pos: Position, kind: FluidKind) -> bool {
        let Some(below) = offset(pos, (0, -1, 0)) else {
            return false;
        };
        let target = block(w, below);
        can_pass_through(block(w, pos), target)
            && (fluid_state(target).kind == kind || can_hold_fluid(target, kind))
    }
    fn slope(
        &self,
        w: &impl FluidWorld,
        pos: Position,
        prev: Position,
        kind: FluidKind,
        depth: u32,
        limit: u32,
    ) -> u32 {
        let mut best = u32::MAX;
        for d in SIDES {
            let Some(n) = offset(pos, d) else { continue };
            if n == prev || !self.can_explore(w, pos, n, kind) {
                continue;
            }
            if self.hole(w, n, kind) {
                return depth;
            }
            if depth < limit {
                best = best.min(self.slope(w, n, pos, kind, depth + 1, limit));
            }
        }
        best
    }
}
