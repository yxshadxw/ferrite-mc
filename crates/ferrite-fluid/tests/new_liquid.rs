use ferrite_data::blocks;
use ferrite_fluid::{
    FluidKind, FluidWorld, can_hold_fluid, can_pass_through, fluid_state, get_new_liquid,
};
use std::collections::HashMap;

struct Grid {
    cells: HashMap<(i32, i32, i32), u32>,
    hot: bool,
}
impl FluidWorld for Grid {
    fn block(&self, x: i32, y: i32, z: i32) -> u32 {
        *self
            .cells
            .get(&(x, y, z))
            .unwrap_or(&u32::from(blocks::default_state("air").unwrap()))
    }
    fn set_block(&mut self, x: i32, y: i32, z: i32, state: u32) {
        self.cells.insert((x, y, z), state);
    }
    fn ultrawarm(&self) -> bool {
        self.hot
    }
}
fn grid(hot: bool) -> Grid {
    Grid {
        cells: HashMap::new(),
        hot,
    }
}
fn block(name: &str) -> u32 {
    u32::from(blocks::default_state(name).unwrap())
}
fn state(name: &str, level: u8) -> u32 {
    u32::from(blocks::state_id(name, &[("level", &level.to_string())]).unwrap())
}

#[test]
fn strongest_neighbor_loses_dimension_dependent_amount() {
    let mut w = grid(false);
    w.set_block(1, 0, 0, state("water", 2));
    assert_eq!(
        get_new_liquid(&w, 0, 0, 0, FluidKind::Water, false).amount,
        5
    );
    w.set_block(1, 0, 0, state("lava", 2));
    assert_eq!(
        get_new_liquid(&w, 0, 0, 0, FluidKind::Lava, false).amount,
        4
    );
    w.hot = true;
    assert_eq!(
        get_new_liquid(&w, 0, 0, 0, FluidKind::Lava, false).amount,
        5
    );
}
#[test]
fn two_sources_form_water_source_only_with_support() {
    let mut w = grid(false);
    w.set_block(-1, 0, 0, block("water"));
    w.set_block(1, 0, 0, block("water"));
    assert!(!get_new_liquid(&w, 0, 0, 0, FluidKind::Water, false).source);
    w.set_block(0, -1, 0, block("stone"));
    assert!(get_new_liquid(&w, 0, 0, 0, FluidKind::Water, false).source);
    w.set_block(0, -1, 0, block("water"));
    assert!(get_new_liquid(&w, 0, 0, 0, FluidKind::Water, false).source);
}
#[test]
fn lava_source_creation_obeys_conversion_rule() {
    let mut w = grid(false);
    w.set_block(-1, 0, 0, block("lava"));
    w.set_block(1, 0, 0, block("lava"));
    w.set_block(0, -1, 0, block("stone"));
    assert!(!get_new_liquid(&w, 0, 0, 0, FluidKind::Lava, false).source);
    assert!(get_new_liquid(&w, 0, 0, 0, FluidKind::Lava, true).source);
}
#[test]
fn fluid_above_produces_falling_state() {
    let mut w = grid(false);
    w.set_block(0, 1, 0, state("water", 5));
    let result = get_new_liquid(&w, 0, 0, 0, FluidKind::Water, false);
    assert_eq!(
        (result.kind, result.amount, result.falling, result.source),
        (FluidKind::Water, 8, true, false)
    );
    w.set_block(0, 1, 0, block("stone"));
    assert_eq!(
        get_new_liquid(&w, 0, 0, 0, FluidKind::Water, false),
        fluid_state(block("air"))
    );
}
#[test]
fn block_helpers_respect_solid_and_waterloggable_states() {
    let slab = u32::from(
        blocks::state_id(
            "stone_slab",
            &[("type", "bottom"), ("waterlogged", "false")],
        )
        .unwrap(),
    );
    assert!(can_hold_fluid(block("air"), FluidKind::Water));
    assert!(can_hold_fluid(slab, FluidKind::Water));
    assert!(!can_hold_fluid(block("stone"), FluidKind::Water));
    assert!(!can_pass_through(block("stone"), block("water")));
    assert!(can_pass_through(block("air"), block("water")));
}

#[test]
fn source_conversion_precedes_water_above_and_solid_walls_block_flow() {
    let mut w = grid(false);
    w.set_block(-1, 0, 0, block("water"));
    w.set_block(1, 0, 0, block("water"));
    w.set_block(0, -1, 0, block("stone"));
    w.set_block(0, 1, 0, block("water"));
    assert!(get_new_liquid(&w, 0, 0, 0, FluidKind::Water, false).source);
    w.set_block(0, 0, 0, block("stone"));
    assert_eq!(
        get_new_liquid(&w, 0, 0, 0, FluidKind::Water, false).kind,
        FluidKind::Empty
    );
}
