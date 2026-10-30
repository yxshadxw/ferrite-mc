//! Vanilla 26.3 game rule types, defaults, and bounds (GameRules.java).
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleValue {
    Boolean(bool),
    Integer(i32),
}
const SPECS: &[(&str, RuleValue, i32, i32)] = &[
    ("advance_time", RuleValue::Boolean(true), 0, 0),
    ("advance_weather", RuleValue::Boolean(true), 0, 0),
    (
        "allow_entering_nether_using_portals",
        RuleValue::Boolean(true),
        0,
        0,
    ),
    ("block_drops", RuleValue::Boolean(true), 0, 0),
    ("block_explosion_drop_decay", RuleValue::Boolean(true), 0, 0),
    ("command_blocks_work", RuleValue::Boolean(true), 0, 0),
    ("command_block_output", RuleValue::Boolean(true), 0, 0),
    ("drowning_damage", RuleValue::Boolean(true), 0, 0),
    ("elytra_movement_check", RuleValue::Boolean(true), 0, 0),
    (
        "ender_pearls_vanish_on_death",
        RuleValue::Boolean(true),
        0,
        0,
    ),
    ("entity_drops", RuleValue::Boolean(true), 0, 0),
    ("fall_damage", RuleValue::Boolean(true), 0, 0),
    ("fire_damage", RuleValue::Boolean(true), 0, 0),
    (
        "fire_spread_radius_around_player",
        RuleValue::Integer(128),
        -1,
        2147483647,
    ),
    ("forgive_dead_players", RuleValue::Boolean(true), 0, 0),
    ("freeze_damage", RuleValue::Boolean(true), 0, 0),
    ("global_sound_events", RuleValue::Boolean(true), 0, 0),
    ("immediate_respawn", RuleValue::Boolean(false), 0, 0),
    ("keep_inventory", RuleValue::Boolean(false), 0, 0),
    ("lava_source_conversion", RuleValue::Boolean(false), 0, 0),
    ("limited_crafting", RuleValue::Boolean(false), 0, 0),
    ("locator_bar", RuleValue::Boolean(true), 0, 0),
    ("log_admin_commands", RuleValue::Boolean(true), 0, 0),
    (
        "max_block_modifications",
        RuleValue::Integer(32768),
        1,
        2147483647,
    ),
    (
        "max_command_forks",
        RuleValue::Integer(65536),
        0,
        2147483647,
    ),
    (
        "max_command_sequence_length",
        RuleValue::Integer(65536),
        0,
        2147483647,
    ),
    ("max_entity_cramming", RuleValue::Integer(24), 0, 2147483647),
    ("max_minecart_speed", RuleValue::Integer(8), 1, 1000),
    ("max_snow_accumulation_height", RuleValue::Integer(1), 0, 8),
    ("mob_drops", RuleValue::Boolean(true), 0, 0),
    ("mob_explosion_drop_decay", RuleValue::Boolean(true), 0, 0),
    ("mob_griefing", RuleValue::Boolean(true), 0, 0),
    (
        "natural_health_regeneration",
        RuleValue::Boolean(true),
        0,
        0,
    ),
    ("player_movement_check", RuleValue::Boolean(true), 0, 0),
    (
        "players_nether_portal_creative_delay",
        RuleValue::Integer(0),
        0,
        2147483647,
    ),
    (
        "players_nether_portal_default_delay",
        RuleValue::Integer(80),
        0,
        2147483647,
    ),
    (
        "players_sleeping_percentage",
        RuleValue::Integer(100),
        0,
        2147483647,
    ),
    (
        "projectiles_can_break_blocks",
        RuleValue::Boolean(true),
        0,
        0,
    ),
    ("pvp", RuleValue::Boolean(true), 0, 0),
    ("raids", RuleValue::Boolean(true), 0, 0),
    ("random_tick_speed", RuleValue::Integer(3), 0, 2147483647),
    ("reduced_debug_info", RuleValue::Boolean(false), 0, 0),
    ("respawn_radius", RuleValue::Integer(10), 0, 2147483647),
    ("send_command_feedback", RuleValue::Boolean(true), 0, 0),
    ("show_advancement_messages", RuleValue::Boolean(true), 0, 0),
    ("show_death_messages", RuleValue::Boolean(true), 0, 0),
    ("spawner_blocks_work", RuleValue::Boolean(true), 0, 0),
    ("spawn_mobs", RuleValue::Boolean(true), 0, 0),
    ("spawn_monsters", RuleValue::Boolean(true), 0, 0),
    ("spawn_patrols", RuleValue::Boolean(true), 0, 0),
    ("spawn_phantoms", RuleValue::Boolean(true), 0, 0),
    ("spawn_wandering_traders", RuleValue::Boolean(true), 0, 0),
    ("spawn_wardens", RuleValue::Boolean(true), 0, 0),
    ("spectators_generate_chunks", RuleValue::Boolean(true), 0, 0),
    ("spread_vines", RuleValue::Boolean(true), 0, 0),
    ("tnt_explodes", RuleValue::Boolean(true), 0, 0),
    ("tnt_explosion_drop_decay", RuleValue::Boolean(false), 0, 0),
    ("universal_anger", RuleValue::Boolean(false), 0, 0),
    ("water_source_conversion", RuleValue::Boolean(true), 0, 0),
];
fn canonical(name: &str) -> &str {
    match name.strip_prefix("minecraft:").unwrap_or(name) {
        "randomTickSpeed" => "random_tick_speed",
        "doDaylightCycle" => "advance_time",
        "doMobSpawning" => "spawn_mobs",
        other => other,
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameRules {
    values: BTreeMap<String, RuleValue>,
}
impl Default for GameRules {
    fn default() -> Self {
        Self {
            values: SPECS
                .iter()
                .map(|(name, value, _, _)| ((*name).to_owned(), *value))
                .collect(),
        }
    }
}
impl GameRules {
    pub fn get(&self, name: &str) -> Option<RuleValue> {
        self.values.get(canonical(name)).copied()
    }
    pub fn set(&mut self, name: &str, value: &str) -> Result<RuleValue, String> {
        let name = canonical(name);
        let (_, kind, min, max) = SPECS
            .iter()
            .find(|(key, _, _, _)| *key == name)
            .ok_or_else(|| format!("Unknown game rule: {name}"))?;
        let parsed = match kind {
            RuleValue::Boolean(_) => RuleValue::Boolean(
                value
                    .parse::<bool>()
                    .map_err(|_| "Expected true or false".to_owned())?,
            ),
            RuleValue::Integer(_) => {
                let v = value
                    .parse::<i32>()
                    .map_err(|_| "Expected an integer".to_owned())?;
                if v < *min || v > *max {
                    return Err(format!("Value must be between {min} and {max}"));
                }
                RuleValue::Integer(v)
            }
        };
        self.values.insert(name.to_owned(), parsed);
        Ok(parsed)
    }
}
