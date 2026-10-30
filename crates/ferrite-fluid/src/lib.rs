//! Vanilla fluid-state conversion and base tick delays.
//!
//! Fluid properties are extracted from the 26.3 server by ferrite-data.
//! Vanilla `LiquidBlock` maps block levels 8..=15 to the same falling fluid
//! state; converting that state back therefore produces canonical level 8.

use ferrite_data::{block_props, blocks};

pub trait FluidWorld {
    fn block(&self, x: i32, y: i32, z: i32) -> u32;
    fn set_block(&mut self, x: i32, y: i32, z: i32, state: u32);
    fn ultrawarm(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FluidKind {
    Water,
    Lava,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FluidState {
    pub kind: FluidKind,
    /// Zero for empty; otherwise 1 through 8.
    pub amount: u8,
    pub falling: bool,
    pub source: bool,
}

/// Decode a vanilla block state. Unknown IDs do not contain fluid.
pub fn fluid_state(state_id: u32) -> FluidState {
    let empty = FluidState {
        kind: FluidKind::Empty,
        amount: 0,
        falling: false,
        source: false,
    };
    let Ok(id) = u16::try_from(state_id) else {
        return empty;
    };
    if usize::from(id) >= block_props::STATE_COUNT {
        return empty;
    }
    let Some((kind, amount, falling)) = block_props::get(id).fluid else {
        return empty;
    };
    let (kind, source) = match kind {
        block_props::FluidKind::Water => (FluidKind::Water, true),
        block_props::FluidKind::FlowingWater => (FluidKind::Water, false),
        block_props::FluidKind::Lava => (FluidKind::Lava, true),
        block_props::FluidKind::FlowingLava => (FluidKind::Lava, false),
        block_props::FluidKind::Other(_) => return empty,
    };
    FluidState {
        kind,
        amount,
        falling,
        source,
    }
}

/// Convert to a standalone fluid block, or air for empty. A waterlogged
/// container cannot be recovered from its fluid alone.
///
/// Panics if the state is inconsistent or cannot be represented as a block.
pub fn block_for(fluid: FluidState) -> u32 {
    let name = match fluid.kind {
        FluidKind::Empty => {
            assert!(fluid.amount == 0 && !fluid.source && !fluid.falling);
            return u32::from(blocks::default_state("air").expect("vanilla air"));
        }
        FluidKind::Water => "water",
        FluidKind::Lava => "lava",
    };
    assert!((1..=8).contains(&fluid.amount));
    assert!(!fluid.source || (fluid.amount == 8 && !fluid.falling));
    assert!(fluid.source || fluid.falling || fluid.amount < 8);
    let level = if fluid.source {
        0
    } else {
        8 - fluid.amount + if fluid.falling { 8 } else { 0 }
    };
    u32::from(
        blocks::state_id(name, &[("level", &level.to_string())]).expect("vanilla fluid level"),
    )
}

/// Base fluid tick delay in game ticks; empty has no scheduled fluid tick.
pub const fn tick_delay(kind: FluidKind, ultrawarm: bool) -> u32 {
    match kind {
        FluidKind::Water => 5,
        FluidKind::Lava if ultrawarm => 10,
        FluidKind::Lava => 30,
        FluidKind::Empty => 0,
    }
}

fn props(state: u32) -> Option<&'static block_props::BlockProps> {
    let id = u16::try_from(state).ok()?;
    (usize::from(id) < block_props::STATE_COUNT).then(|| block_props::get(id))
}

/// Approximation of `FlowingFluid.java#canHoldFluid`: replaceable blocks and
/// waterloggable blocks can accept water. Other block-specific container rules
/// are not yet modeled, and existing fluids are accepted only of the same kind.
pub fn can_hold_fluid(state: u32, kind: FluidKind) -> bool {
    if kind == FluidKind::Empty {
        return false;
    }
    let existing = fluid_state(state);
    if existing.kind != FluidKind::Empty {
        return existing.kind == kind;
    }
    let Some(p) = props(state) else {
        return false;
    };
    if p.replaceable {
        return true;
    }
    kind == FluidKind::Water
        && u16::try_from(state)
            .ok()
            .and_then(blocks::state)
            .is_some_and(|(_, properties)| {
                properties
                    .iter()
                    .any(|&(name, value)| name == "waterlogged" && value == "false")
            })
}

/// Approximation of `FlowingFluid.java#canPassThroughWall`: a full collision cube
/// blocks every face. Partial shapes and their direction-specific occlusion are
/// treated as passable, including combinations whose faces would jointly seal.
pub fn can_pass_through(from: u32, to: u32) -> bool {
    props(from).is_some_and(|p| !p.full_collision) && props(to).is_some_and(|p| !p.full_collision)
}

/// Compute `FlowingFluid.java#getNewLiquid` without changing the world. Source
/// support uses `blocks_motion` as a conservative approximation of vanilla's
/// `BlockState.isSolid`; face occlusion uses [`can_pass_through`].
/// `lava_conversion` is the lava source-conversion game rule.
pub fn get_new_liquid(
    world: &impl FluidWorld,
    x: i32,
    y: i32,
    z: i32,
    kind: FluidKind,
    lava_conversion: bool,
) -> FluidState {
    let empty = FluidState {
        kind: FluidKind::Empty,
        amount: 0,
        falling: false,
        source: false,
    };
    if kind == FluidKind::Empty {
        return empty;
    }
    let current = world.block(x, y, z);
    let mut max_amount = 0;
    let mut sources = 0;
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let (Some(nx), Some(nz)) = (x.checked_add(dx), z.checked_add(dz)) else {
            continue;
        };
        let adjacent = world.block(nx, y, nz);
        let fluid = fluid_state(adjacent);
        if fluid.kind == kind && can_pass_through(current, adjacent) {
            max_amount = max_amount.max(fluid.amount);
            sources += u8::from(fluid.source);
        }
    }
    if sources >= 2
        && (kind == FluidKind::Water || lava_conversion)
        && let Some(below_y) = y.checked_sub(1)
    {
        let below = world.block(x, below_y, z);
        let below_fluid = fluid_state(below);
        if props(below).is_some_and(|p| p.blocks_motion)
            || (below_fluid.kind == kind && below_fluid.source)
        {
            return FluidState {
                kind,
                amount: 8,
                falling: false,
                source: true,
            };
        }
    }
    if let Some(above_y) = y.checked_add(1) {
        let above = world.block(x, above_y, z);
        if fluid_state(above).kind == kind && can_pass_through(current, above) {
            return FluidState {
                kind,
                amount: 8,
                falling: true,
                source: false,
            };
        }
    }
    let drop_off = if kind == FluidKind::Lava && !world.ultrawarm() {
        2
    } else {
        1
    };
    let amount = max_amount.saturating_sub(drop_off);
    if amount == 0 {
        empty
    } else {
        FluidState {
            kind,
            amount,
            falling: false,
            source: false,
        }
    }
}

mod tick;
pub use tick::{FluidTicks, Position};
