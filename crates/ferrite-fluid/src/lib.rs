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
