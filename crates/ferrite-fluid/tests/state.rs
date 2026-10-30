use ferrite_data::blocks;
use ferrite_fluid::{FluidKind, FluidState, block_for, fluid_state, tick_delay};

#[test]
fn vanilla_block_levels_decode_and_encode_as_fluid_states() {
    for (name, kind) in [("water", FluidKind::Water), ("lava", FluidKind::Lava)] {
        for level in 0..16 {
            let id = u32::from(blocks::state_id(name, &[("level", &level.to_string())]).unwrap());
            let state = fluid_state(id);
            assert_eq!(state.kind, kind);
            assert_eq!(state.source, level == 0);
            assert_eq!(state.falling, level >= 8);
            assert_eq!(
                state.amount,
                if level == 0 || level >= 8 {
                    8
                } else {
                    8 - level
                }
            );
            assert_eq!(fluid_state(block_for(state)), state);
        }
    }
}

#[test]
fn waterlogged_slab_and_air_have_expected_fluid() {
    let slab = u32::from(
        blocks::state_id("stone_slab", &[("type", "bottom"), ("waterlogged", "true")]).unwrap(),
    );
    assert_eq!(
        fluid_state(slab),
        FluidState {
            kind: FluidKind::Water,
            amount: 8,
            falling: false,
            source: true
        }
    );
    let air = u32::from(blocks::default_state("air").unwrap());
    assert_eq!(
        fluid_state(air),
        FluidState {
            kind: FluidKind::Empty,
            amount: 0,
            falling: false,
            source: false
        }
    );
    assert_eq!(block_for(fluid_state(air)), air);
}

#[test]
fn water_and_lava_tick_delays_depend_on_dimension() {
    assert_eq!(tick_delay(FluidKind::Water, false), 5);
    assert_eq!(tick_delay(FluidKind::Water, true), 5);
    assert_eq!(tick_delay(FluidKind::Lava, false), 30);
    assert_eq!(tick_delay(FluidKind::Lava, true), 10);
}

#[test]
fn falling_amounts_encode_legacy_levels_and_unknown_ids_are_empty() {
    for (name, kind) in [("water", FluidKind::Water), ("lava", FluidKind::Lava)] {
        for amount in 1..=8 {
            let state = FluidState {
                kind,
                amount,
                falling: true,
                source: false,
            };
            let level = (16 - amount).to_string();
            assert_eq!(
                block_for(state),
                u32::from(blocks::state_id(name, &[("level", &level)]).unwrap())
            );
        }
    }
    assert_eq!(fluid_state(u32::MAX).kind, FluidKind::Empty);
}
