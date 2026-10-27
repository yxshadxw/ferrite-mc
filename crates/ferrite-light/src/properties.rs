//! Light properties derived from the generated block-state names and properties.
//!
//! The generator report does not contain light emission, attenuation or voxel
//! shapes. This table covers common states; unknown states conservatively default
//! to opaque. TODO: replace the approximations with a complete vanilla state and
//! face-shape export before claiming vanilla parity. In particular, many
//! decorative blocks require exact voxel unions.
use crate::classifications::{OPACITY_0, OPACITY_1};
use std::sync::OnceLock;

/// A face is a four-quadrant mask, in directions west, east, down, up, north, south.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightProperties {
    pub emission: u8,
    pub opacity: u8,
    pub faces: [u8; 6],
}
impl LightProperties {
    pub const AIR: Self = Self {
        emission: 0,
        opacity: 0,
        faces: [0; 6],
    };
    pub const OPAQUE: Self = Self {
        emission: 0,
        opacity: 15,
        faces: [15; 6],
    };
}
/// Common vanilla states, looked up once and cached by global state ID.
pub fn vanilla_properties(id: u16) -> LightProperties {
    static TABLE: OnceLock<Vec<LightProperties>> = OnceLock::new();
    TABLE
        .get_or_init(|| {
            (0..ferrite_data::blocks::STATE_COUNT)
                .map(|id| classify(id as u16))
                .collect()
        })
        .get(id as usize)
        .copied()
        .unwrap_or(LightProperties::OPAQUE)
}
fn classify(id: u16) -> LightProperties {
    let Some((name, props)) = ferrite_data::blocks::state(id) else {
        return LightProperties::OPAQUE;
    };
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    let value = |key: &str| props.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
    let enabled = || value("lit").is_none_or(|v| v == "true");
    let emission = match name {
        "torch" | "wall_torch" | "copper_torch" | "copper_wall_torch" | "end_rod" => 14,
        "cave_vines" | "cave_vines_plant" if value("berries") == Some("true") => 14,
        "redstone_ore" | "deepslate_redstone_ore" if enabled() => 9,
        "crying_obsidian" => 10,
        "fire" | "end_portal" => 15,
        "soul_fire" => 10,
        "nether_portal" => 11,
        "brewing_stand" => 1,
        "enchanting_table" | "ender_chest" => 7,
        "firefly_bush" | "medium_amethyst_bud" => 2,
        "small_amethyst_bud" => 1,
        "large_amethyst_bud" => 4,
        "amethyst_cluster" => 5,
        "sculk_catalyst" => 6,
        "sculk_sensor" | "calibrated_sculk_sensor" | "end_portal_frame" | "dragon_egg" => 1,
        "end_gateway" => 15,
        "vault" => {
            if value("vault_state") == Some("inactive") {
                6
            } else {
                12
            }
        }
        "trial_spawner" => match value("trial_spawner_state") {
            Some("waiting_for_players") => 4,
            Some("active" | "waiting_for_reward_ejection" | "ejecting_reward") => 8,
            _ => 0,
        },
        "respawn_anchor" => value("charges")
            .and_then(|v| v.parse::<u8>().ok())
            .map_or(0, |v| [0, 3, 7, 11, 15][usize::from(v.min(4))]),
        n if n.ends_with("copper_bulb") && enabled() => {
            if n.contains("oxidized") {
                4
            } else if n.contains("weathered") {
                8
            } else if n.contains("exposed") {
                12
            } else {
                15
            }
        }
        n if n.ends_with("copper_lantern") => 15,
        "soul_torch" | "soul_wall_torch" | "soul_lantern" => 10,
        "redstone_torch" | "redstone_wall_torch" if enabled() => 7,
        "glowstone"
        | "sea_lantern"
        | "shroomlight"
        | "jack_o_lantern"
        | "lantern"
        | "beacon"
        | "conduit"
        | "lava"
        | "lava_cauldron"
        | "ochre_froglight"
        | "verdant_froglight"
        | "pearlescent_froglight" => 15,
        "redstone_lamp" if enabled() => 15,
        "campfire" if enabled() => 15,
        "soul_campfire" if enabled() => 10,
        "furnace" | "blast_furnace" | "smoker" if enabled() => 13,
        "magma_block" => 3,
        "glow_lichen"
            if ["down", "up", "north", "south", "east", "west"]
                .iter()
                .any(|face| value(face) == Some("true")) =>
        {
            7
        }
        "brown_mushroom" => 1,
        "light" => value("level").and_then(|v| v.parse().ok()).unwrap_or(15),
        "sea_pickle" if value("waterlogged") == Some("true") => value("pickles")
            .and_then(|v| v.parse::<u8>().ok())
            .map_or(6, |v| 3 * v + 3),
        n if (n.ends_with("candle") || n.ends_with("candle_cake")) && enabled() => {
            value("candles")
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(1)
                * 3
        }
        _ => 0,
    };
    let mut p = if name.ends_with("_stairs") {
        LightProperties {
            faces: stair_faces(
                value("facing").unwrap_or("north"),
                value("half").unwrap_or("bottom"),
                value("shape").unwrap_or("straight"),
            ),
            ..LightProperties::AIR
        }
    } else if name.ends_with("_slab") {
        match value("type") {
            Some("double") => LightProperties::OPAQUE,
            Some("top") => LightProperties {
                faces: [12, 12, 0, 15, 12, 12],
                ..LightProperties::AIR
            },
            _ => LightProperties {
                faces: [3, 3, 15, 0, 3, 3],
                ..LightProperties::AIR
            },
        }
    } else if OPACITY_0.binary_search(&name).is_ok() {
        LightProperties::AIR
    } else if OPACITY_1.binary_search(&name).is_ok() {
        LightProperties {
            opacity: 1,
            ..LightProperties::AIR
        }
    } else {
        LightProperties::OPAQUE
    };
    if matches!(name, "piston" | "sticky_piston") && value("extended") == Some("true") {
        p = LightProperties::AIR;
    }
    if name == "snow" && value("layers") == Some("8") {
        p = LightProperties::OPAQUE;
    }
    if matches!(name, "resin_clump" | "sculk_vein")
        && value("waterlogged") == Some("false")
        && ["down", "up", "north", "south", "east", "west"]
            .iter()
            .any(|face| value(face) == Some("true"))
    {
        p.opacity = 0;
    }
    if value("waterlogged") == Some("true") && p.opacity != 15 && !name.ends_with("copper_grate") {
        p.opacity = 1;
    }
    p.emission = emission;
    p
}

// Stair light shapes consist of a half-block and a second-layer straight,
// inner-corner, or outer-corner set of half-block quadrants.
fn stair_faces(facing: &str, half: &str, shape: &str) -> [u8; 6] {
    let mut faces = [0u8; 6];
    for y in 0..2 {
        for z in 0..2 {
            for x in 0..2 {
                let forward = match facing {
                    "south" => z == 1,
                    "west" => x == 0,
                    "east" => x == 1,
                    _ => z == 0,
                };
                let left = match facing {
                    "south" => x == 1,
                    "west" => z == 1,
                    "east" => z == 0,
                    _ => x == 0,
                };
                let upper = match shape {
                    "inner_left" => forward || left,
                    "inner_right" => forward || !left,
                    "outer_left" => forward && left,
                    "outer_right" => forward && !left,
                    _ => forward,
                };
                let occupied = if half == "top" {
                    y == 1 || upper
                } else {
                    y == 0 || upper
                };
                if !occupied {
                    continue;
                }
                if x == 0 {
                    faces[0] |= 1 << (y * 2 + z);
                }
                if x == 1 {
                    faces[1] |= 1 << (y * 2 + z);
                }
                if y == 0 {
                    faces[2] |= 1 << (z * 2 + x);
                }
                if y == 1 {
                    faces[3] |= 1 << (z * 2 + x);
                }
                if z == 0 {
                    faces[4] |= 1 << (y * 2 + x);
                }
                if z == 1 {
                    faces[5] |= 1 << (y * 2 + x);
                }
            }
        }
    }
    faces
}
