pub fn break_ticks(state: u16) -> Option<u32> {
    let props = ferrite_data::block_props::get(state);
    if props.hardness < 0.0 {
        return None;
    }
    if props.hardness == 0.0 {
        return Some(1);
    }
    let divisor = if props.requires_correct_tool {
        100.0
    } else {
        30.0
    };
    Some((f64::from(props.hardness) * divisor).ceil().max(1.0) as u32)
}

pub fn placement_state(
    item_id: i32,
    face: i32,
    cursor_y: f32,
    yaw: f32,
    replaced: u16,
) -> Option<u16> {
    let name = ferrite_data::registries::entries("minecraft:item")?
        .iter()
        .find(|name| ferrite_data::registries::id("minecraft:item", name) == Some(item_id))?;
    let default = ferrite_data::blocks::default_state(name)?;
    let (_, defaults) = ferrite_data::blocks::state(default)?;
    let mut properties = defaults.to_vec();
    let water = ferrite_data::block_props::get(replaced)
        .fluid
        .is_some_and(|(kind, _, _)| {
            matches!(
                kind,
                ferrite_data::block_props::FluidKind::Water
                    | ferrite_data::block_props::FluidKind::FlowingWater
            )
        });
    let axis = match face {
        0 | 1 => "y",
        2 | 3 => "z",
        4 | 5 => "x",
        _ => return None,
    };
    let facing = match ((yaw / 90.0).round() as i32).rem_euclid(4) {
        0 => "north",
        1 => "east",
        2 => "south",
        _ => "west",
    };
    let upper = face == 0 || (face != 1 && cursor_y > 0.5);
    for (key, value) in &mut properties {
        *value = match *key {
            "axis" => axis,
            "facing" if name.contains("stairs") || name.contains("door") => facing,
            "half" if name.contains("stairs") => {
                if upper {
                    "top"
                } else {
                    "bottom"
                }
            }
            "half" if name.contains("door") => "lower",
            "type" if name.contains("slab") => {
                if upper {
                    "top"
                } else {
                    "bottom"
                }
            }
            "waterlogged" => {
                if water {
                    "true"
                } else {
                    "false"
                }
            }
            _ => *value,
        };
    }
    ferrite_data::blocks::state_id(name, &properties).or(Some(default))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tool_less_stone_and_log_axis() {
        let stone = ferrite_data::blocks::default_state("stone").unwrap();
        assert_eq!(break_ticks(stone), Some(150));
        let item = ferrite_data::registries::id("minecraft:item", "minecraft:oak_log").unwrap();
        let state = placement_state(item, 5, 0.5, 0.0, 0).unwrap();
        assert_eq!(
            ferrite_data::blocks::state(state)
                .unwrap()
                .1
                .iter()
                .find(|(k, _)| *k == "axis")
                .unwrap()
                .1,
            "x"
        );
    }
}
