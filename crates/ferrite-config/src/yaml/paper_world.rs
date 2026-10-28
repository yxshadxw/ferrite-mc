//! Paper world configuration.
//!
//! [`WorldSettings`] is the typed model for `config/paper-world-defaults.yml` and for a
//! per-world `<world>/paper-world.yml`. A per-world file only needs the keys it overrides;
//! [`crate::FerriteConfig::world_settings`] meshes it on top of the defaults.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ConfigFileKind, KeyComment, MaybeInt, YamlConfig};

/// Resolved world settings: the world defaults, with any per-world overrides applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WorldSettings {
    /// Schema version. Paper writes `_version_`; older configs may use `config-version`.
    #[serde(rename = "_version_", alias = "config-version")]
    pub config_version: u32,
    /// Anti-cheat settings.
    pub anticheat: WorldAnticheat,
    /// Chunk loading and saving.
    pub chunks: ChunksSettings,
    /// Collision behavior.
    pub collisions: WorldCollisions,
    /// Command block behavior.
    pub command_blocks: CommandBlocks,
    /// Entity behavior and spawning.
    pub entities: Entities,
    /// World environment behavior.
    pub environment: Environment,
    /// Bug fixes that change vanilla behavior.
    pub fixes: Fixes,
    /// Hopper optimizations.
    pub hopper: HopperSettings,
    /// Map behavior.
    pub maps: MapSettings,
    /// Maximum natural growth heights.
    pub max_growth_height: MaxGrowthHeight,
    /// Assorted gameplay settings.
    pub misc: WorldMisc,
    /// Scoreboard behavior.
    pub scoreboards: WorldScoreboards,
    /// Spawn behavior.
    pub spawn: SpawnSettings,
    /// Tick rate overrides.
    pub tick_rates: TickRates,
    /// Settings Paper does not officially support.
    pub unsupported_settings: WorldUnsupportedSettings,
}

impl Default for WorldSettings {
    fn default() -> Self {
        Self {
            config_version: ConfigFileKind::PaperWorldDefaults.supported_version(),
            anticheat: WorldAnticheat::default(),
            chunks: ChunksSettings::default(),
            collisions: WorldCollisions::default(),
            command_blocks: CommandBlocks::default(),
            entities: Entities::default(),
            environment: Environment::default(),
            fixes: Fixes::default(),
            hopper: HopperSettings::default(),
            maps: MapSettings::default(),
            max_growth_height: MaxGrowthHeight::default(),
            misc: WorldMisc::default(),
            scoreboards: WorldScoreboards::default(),
            spawn: SpawnSettings::default(),
            tick_rates: TickRates::default(),
            unsupported_settings: WorldUnsupportedSettings::default(),
        }
    }
}

/// `anticheat`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WorldAnticheat {
    /// Anti-xray.
    pub anti_xray: AntiXray,
}

/// `anticheat.anti-xray`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct AntiXray {
    /// Enable anti-xray.
    pub enabled: bool,
    /// Engine mode 1, 2, or 3.
    pub engine_mode: u8,
    /// Blocks that are hidden from clients.
    pub hidden_blocks: Vec<String>,
    /// Whether lava obfuscates blocks.
    pub lava_obscures: bool,
    /// Highest Y level anti-xray applies to.
    pub max_block_height: i32,
    /// Blocks used as replacements.
    pub replacement_blocks: Vec<String>,
    /// Radius for real block updates when a player interacts.
    pub update_radius: i32,
    /// Allow `paper.antixray.bypass`.
    pub use_permission: bool,
}

impl Default for AntiXray {
    fn default() -> Self {
        Self {
            enabled: false,
            engine_mode: 1,
            hidden_blocks: [
                "copper_ore",
                "deepslate_copper_ore",
                "raw_copper_block",
                "gold_ore",
                "deepslate_gold_ore",
                "iron_ore",
                "deepslate_iron_ore",
                "raw_iron_block",
                "coal_ore",
                "deepslate_coal_ore",
                "lapis_ore",
                "deepslate_lapis_ore",
                "mossy_cobblestone",
                "obsidian",
                "chest",
                "diamond_ore",
                "deepslate_diamond_ore",
                "redstone_ore",
                "deepslate_redstone_ore",
                "clay",
                "emerald_ore",
                "deepslate_emerald_ore",
                "ender_chest",
            ]
            .iter()
            .map(|block| (*block).to_string())
            .collect(),
            lava_obscures: false,
            max_block_height: 64,
            replacement_blocks: ["stone", "oak_planks", "deepslate"]
                .iter()
                .map(|block| (*block).to_string())
                .collect(),
            update_radius: 2,
            use_permission: false,
        }
    }
}

/// `chunks`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ChunksSettings {
    /// Save interval in ticks; `default` uses `ticks-per.autosave` from bukkit.yml.
    pub auto_save_interval: MaybeInt,
    /// Delay chunk unloads by a duration such as `10s`.
    pub delay_chunk_unloads_by: String,
    /// Per-entity-type save/load limits per chunk; -1 disables the limit.
    pub entity_per_chunk_save_limit: BTreeMap<String, i32>,
    /// Fixed chunk inhabited time; -1 leaves it dynamic.
    pub fixed_chunk_inhabited_time: i64,
    /// Flush region files when chunks are saved.
    pub flush_regions_on_save: bool,
    /// Maximum chunks auto-saved per tick.
    pub max_auto_save_chunks_per_tick: i32,
    /// Minimum chunks unloaded per tick.
    pub min_chunk_unload_count: i32,
    /// Minimum fraction of eligible chunks unloaded per tick.
    pub min_chunk_unload_fraction: f64,
    /// Prevent players from moving into unloaded chunks.
    pub prevent_moving_into_unloaded_chunks: bool,
}

impl Default for ChunksSettings {
    fn default() -> Self {
        let mut entity_per_chunk_save_limit = BTreeMap::new();
        entity_per_chunk_save_limit.insert("experience_orb".to_string(), -1);
        Self {
            auto_save_interval: MaybeInt::Default,
            delay_chunk_unloads_by: "10s".to_string(),
            entity_per_chunk_save_limit,
            fixed_chunk_inhabited_time: -1,
            flush_regions_on_save: false,
            max_auto_save_chunks_per_tick: 24,
            min_chunk_unload_count: 50,
            min_chunk_unload_fraction: 0.05,
            prevent_moving_into_unloaded_chunks: false,
        }
    }
}

/// `collisions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WorldCollisions {
    /// Allow cramming damage from player collisions.
    pub allow_player_cramming_damage: bool,
    /// Allow vehicles to collide when `only-players-collide` is on.
    pub allow_vehicle_collisions: bool,
    /// Count climbing entities towards the cramming limit.
    pub fix_climbing_bypassing_cramming_rule: bool,
    /// Stop processing collisions after this many entities.
    pub max_entity_collisions: i32,
    /// Only calculate collisions when a player is involved.
    pub only_players_collide: bool,
}

impl Default for WorldCollisions {
    fn default() -> Self {
        Self {
            allow_player_cramming_damage: false,
            allow_vehicle_collisions: true,
            fix_climbing_bypassing_cramming_rule: false,
            max_entity_collisions: 8,
            only_players_collide: false,
        }
    }
}

/// `command-blocks`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct CommandBlocks {
    /// Require command blocks to satisfy both Bukkit and vanilla permission checks.
    pub force_follow_perm_level: bool,
    /// Default vanilla permission level for command blocks.
    pub permissions_level: i32,
}

impl Default for CommandBlocks {
    fn default() -> Self {
        Self {
            force_follow_perm_level: true,
            permissions_level: 2,
        }
    }
}

/// `entities`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Entities {
    /// Armor stand behavior.
    pub armor_stands: ArmorStands,
    /// Mob behavior tweaks.
    pub behavior: EntityBehavior,
    /// Markers.
    pub markers: Markers,
    /// Mob effect immunities.
    pub mob_effects: MobEffects,
    /// Mob spawning.
    pub spawning: EntitySpawning,
}

/// `entities.armor-stands`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ArmorStands {
    /// Perform collision entity lookups for armor stands.
    pub do_collision_entity_lookups: bool,
    /// Tick armor stands.
    pub tick: bool,
}

impl Default for ArmorStands {
    fn default() -> Self {
        Self {
            do_collision_entity_lookups: true,
            tick: true,
        }
    }
}

/// `entities.markers`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Markers {
    /// Tick marker entities.
    pub tick: bool,
}

impl Default for Markers {
    fn default() -> Self {
        Self { tick: true }
    }
}

/// `entities.behavior`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct EntityBehavior {
    /// Allow spiders to climb the world border.
    pub allow_spider_world_border_climbing: bool,
    /// Baby zombie speed modifier.
    pub baby_zombie_movement_modifier: f64,
    /// Cooldown failed beehive releases.
    pub cooldown_failed_beehive_releases: bool,
    /// Allow opening chests with a cat on top.
    pub disable_chest_cat_detection: bool,
    /// Disable the creeper lingering effect.
    pub disable_creeper_lingering_effect: bool,
    /// Disable critical hits in PvP.
    pub disable_player_crits: bool,
    /// Ender dragons always drop a dragon egg.
    pub ender_dragons_death_always_places_dragon_egg: bool,
    /// Maximum experience orb value before merging stops; -1 disables the cap.
    pub experience_merge_max_value: i32,
    /// Remove AI from pigmen spawned through nether portals.
    pub nerf_pigmen_from_nether_portals: bool,
    /// Only merge items on the same Y level.
    pub only_merge_items_horizontally: bool,
    /// Make parrots stay on a moving player's shoulder.
    pub parrots_are_unaffected_by_player_movement: bool,
    /// Do not spawn phantoms for creative players.
    pub phantoms_do_not_spawn_on_creative_players: bool,
    /// Only let phantoms attack players who have not slept.
    pub phantoms_only_attack_insomniacs: bool,
    /// Piglins guard chests.
    pub piglins_guard_chests: bool,
    /// Remove the dragon when it exists without a portal.
    pub should_remove_dragon: bool,
    /// Zombies target turtle eggs.
    pub zombies_target_turtle_eggs: bool,
}

impl Default for EntityBehavior {
    fn default() -> Self {
        Self {
            allow_spider_world_border_climbing: true,
            baby_zombie_movement_modifier: 0.5,
            cooldown_failed_beehive_releases: true,
            disable_chest_cat_detection: false,
            disable_creeper_lingering_effect: false,
            disable_player_crits: false,
            ender_dragons_death_always_places_dragon_egg: false,
            experience_merge_max_value: -1,
            nerf_pigmen_from_nether_portals: false,
            only_merge_items_horizontally: false,
            parrots_are_unaffected_by_player_movement: false,
            phantoms_do_not_spawn_on_creative_players: true,
            phantoms_only_attack_insomniacs: true,
            piglins_guard_chests: true,
            should_remove_dragon: false,
            zombies_target_turtle_eggs: true,
        }
    }
}

/// `entities.mob-effects`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MobEffects {
    /// Per-entity wither immunity.
    pub immune_to_wither_effect: WitherImmunity,
    /// Whether spiders are immune to poison.
    pub spiders_immune_to_poison_effect: bool,
}

impl Default for MobEffects {
    fn default() -> Self {
        Self {
            immune_to_wither_effect: WitherImmunity::default(),
            spiders_immune_to_poison_effect: true,
        }
    }
}

/// `entities.mob-effects.immune-to-wither-effect`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WitherImmunity {
    /// Whether withers are immune to the wither effect.
    pub wither: bool,
    /// Whether wither skeletons are immune to the wither effect.
    pub wither_skeleton: bool,
}

impl Default for WitherImmunity {
    fn default() -> Self {
        Self {
            wither: true,
            wither_skeleton: true,
        }
    }
}

/// `entities.spawning`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct EntitySpawning {
    /// Treat all chunks as slime chunks.
    pub all_chunks_are_slime_chunks: bool,
    /// Count all mobs, including spawner mobs, towards the mob cap.
    pub count_all_mobs_for_spawning: bool,
    /// Duplicate UUID handling.
    pub duplicate_uuid: DuplicateUuid,
    /// Allow iron golems to spawn in the air.
    pub iron_golems_can_spawn_in_air: bool,
    /// Count the mob cap per player.
    pub per_player_mob_spawns: bool,
    /// Wandering trader spawn behavior.
    pub wandering_trader: WanderingTrader,
}

impl Default for EntitySpawning {
    fn default() -> Self {
        Self {
            all_chunks_are_slime_chunks: false,
            count_all_mobs_for_spawning: false,
            duplicate_uuid: DuplicateUuid::default(),
            iron_golems_can_spawn_in_air: false,
            per_player_mob_spawns: true,
            wandering_trader: WanderingTrader::default(),
        }
    }
}

/// `entities.spawning.duplicate-uuid`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct DuplicateUuid {
    /// Resolution mode.
    pub mode: DuplicateUuidMode,
    /// Distance within which duplicates are deleted in `SAFE_REGEN`.
    pub safe_regen_delete_range: i32,
}

impl Default for DuplicateUuid {
    fn default() -> Self {
        Self {
            mode: DuplicateUuidMode::SafeRegen,
            safe_regen_delete_range: 32,
        }
    }
}

/// Duplicate UUID resolution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DuplicateUuidMode {
    /// Regenerate the UUID, deleting the entity if it is close to another.
    SafeRegen,
    /// Delete the entity.
    Delete,
    /// Do nothing.
    Nothing,
    /// Do nothing, but log a warning.
    Warn,
}

/// `entities.spawning.wandering-trader`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WanderingTrader {
    /// Increase to the spawn chance after a failed attempt.
    pub spawn_chance_failure_increment: i32,
    /// Maximum spawn chance.
    pub spawn_chance_max: i32,
    /// Minimum spawn chance.
    pub spawn_chance_min: i32,
    /// Ticks between spawn attempts.
    pub spawn_day_length: i32,
    /// Length of the wander trader spawn minute in ticks.
    pub spawn_minute_length: i32,
}

impl Default for WanderingTrader {
    fn default() -> Self {
        Self {
            spawn_chance_failure_increment: 25,
            spawn_chance_max: 75,
            spawn_chance_min: 25,
            spawn_day_length: 24000,
            spawn_minute_length: 1200,
        }
    }
}

/// `environment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Environment {
    /// Disable knockback from explosions.
    pub disable_explosion_knockback: bool,
    /// Disable ice and snow formation.
    pub disable_ice_and_snow: bool,
    /// Disable thunderstorms.
    pub disable_thunder: bool,
    /// Minimum delay between fire ticks.
    pub fire_tick_delay: i32,
    /// Frosted ice behavior.
    pub frosted_ice: FrostedIce,
    /// Generate bedrock as a single flat layer.
    pub generate_flat_bedrock: bool,
    /// Maximum block ticks processed per tick.
    pub max_block_ticks: i32,
    /// Maximum fluid ticks processed per tick.
    pub max_fluid_ticks: i32,
    /// Y level above which nether players take void damage; `disabled` turns it off.
    pub nether_ceiling_void_damage_height: MaybeInt,
    /// Cache entity lookups during explosions.
    pub optimize_explosions: bool,
    /// Maximum range used when creating a portal.
    pub portal_create_radius: i32,
    /// Maximum range used when searching for an existing portal.
    pub portal_search_radius: i32,
    /// Apply vanilla dimension scaling to the portal search radius.
    pub portal_search_vanilla_dimension_scaling: bool,
    /// Void damage per damage attempt; `disabled` turns void damage off.
    pub void_damage_amount: MaybeInt,
    /// Offset from the minimum build height at which void damage applies.
    pub void_damage_min_build_height_offset: i32,
    /// Speed of water flowing over lava.
    pub water_over_lava_flow_speed: i32,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            disable_explosion_knockback: false,
            disable_ice_and_snow: false,
            disable_thunder: false,
            fire_tick_delay: 30,
            frosted_ice: FrostedIce::default(),
            generate_flat_bedrock: false,
            max_block_ticks: 65536,
            max_fluid_ticks: 65536,
            nether_ceiling_void_damage_height: MaybeInt::Disabled,
            optimize_explosions: false,
            portal_create_radius: 16,
            portal_search_radius: 128,
            portal_search_vanilla_dimension_scaling: true,
            void_damage_amount: MaybeInt::Value(4),
            void_damage_min_build_height_offset: -64,
            water_over_lava_flow_speed: 5,
        }
    }
}

/// `environment.frosted-ice`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FrostedIce {
    /// Frosted ice delay range.
    pub delay: FrostedIceDelay,
    /// Enable frosted ice.
    pub enabled: bool,
}

impl Default for FrostedIce {
    fn default() -> Self {
        Self {
            delay: FrostedIceDelay::default(),
            enabled: true,
        }
    }
}

/// `environment.frosted-ice.delay`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FrostedIceDelay {
    /// Maximum delay.
    pub max: i32,
    /// Minimum delay.
    pub min: i32,
}

impl Default for FrostedIceDelay {
    fn default() -> Self {
        Self { max: 40, min: 20 }
    }
}

/// `fixes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Fixes {
    /// Prevent ender pearls from storing their thrower in unloaded chunks.
    pub disable_unloaded_chunk_enderpearl_exploit: bool,
    /// Height at which falling blocks are removed; `disabled` turns it off.
    pub falling_block_height_nerf: MaybeInt,
    /// Prevent items from merging through walls.
    pub fix_items_merging_through_walls: bool,
    /// Keep primed TNT from moving in flowing water.
    pub prevent_tnt_from_moving_in_water: bool,
    /// Split overstacked loot instead of dropping oversized stacks.
    pub split_overstacked_loot: bool,
    /// Height at which primed TNT is removed; `disabled` turns it off.
    pub tnt_entity_height_nerf: MaybeInt,
}

impl Default for Fixes {
    fn default() -> Self {
        Self {
            disable_unloaded_chunk_enderpearl_exploit: false,
            falling_block_height_nerf: MaybeInt::Disabled,
            fix_items_merging_through_walls: false,
            prevent_tnt_from_moving_in_water: false,
            split_overstacked_loot: true,
            tnt_entity_height_nerf: MaybeInt::Disabled,
        }
    }
}

/// `hopper`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct HopperSettings {
    /// Apply a cooldown when a hopper is full.
    pub cooldown_when_full: bool,
    /// Disable the `InventoryMoveItemEvent` for hoppers.
    pub disable_move_event: bool,
    /// Ignore containers inside occluding blocks.
    pub ignore_occluding_blocks: bool,
}

impl Default for HopperSettings {
    fn default() -> Self {
        Self {
            cooldown_when_full: true,
            disable_move_event: false,
            ignore_occluding_blocks: false,
        }
    }
}

/// `maps`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MapSettings {
    /// Maximum cursors per map.
    pub item_frame_cursor_limit: i32,
    /// Ticks between cursor updates; below 1 disables updates.
    pub item_frame_cursor_update_interval: i32,
}

impl Default for MapSettings {
    fn default() -> Self {
        Self {
            item_frame_cursor_limit: 128,
            item_frame_cursor_update_interval: 10,
        }
    }
}

/// `max-growth-height`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MaxGrowthHeight {
    /// Bamboo height range.
    pub bamboo: GrowthRange,
    /// Maximum cactus height.
    pub cactus: i32,
    /// Maximum sugar cane height.
    pub reeds: i32,
}

impl Default for MaxGrowthHeight {
    fn default() -> Self {
        Self {
            bamboo: GrowthRange::default(),
            cactus: 3,
            reeds: 3,
        }
    }
}

/// A minimum/maximum growth height pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct GrowthRange {
    /// Maximum height.
    pub max: i32,
    /// Minimum height.
    pub min: i32,
}

impl Default for GrowthRange {
    fn default() -> Self {
        Self { max: 16, min: 11 }
    }
}

/// `misc` (world).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WorldMisc {
    /// Allow remote ender dragon respawning.
    pub allow_remote_ender_dragon_respawning: bool,
    /// Block update order used by the `ALTERNATE_CURRENT` redstone implementation.
    pub alternate_current_update_order: AlternateCurrentUpdateOrder,
    /// Never send the end credits.
    pub disable_end_credits: bool,
    /// Ignore shooter velocity when calculating projectile velocity.
    pub disable_relative_projectile_velocity: bool,
    /// Do not interrupt sprinting when the player is attacked.
    pub disable_sprint_interruption_on_attack: bool,
    /// Use legacy (pre-1.21.2) ender pearl behavior.
    pub legacy_ender_pearl_behavior: bool,
    /// Maximum leash distance; `default` uses the vanilla value.
    pub max_leash_distance: MaybeInt,
    /// Redstone implementation.
    pub redstone_implementation: RedstoneImplementation,
    /// Show sign click command failures to players.
    pub show_sign_click_command_failure_msgs_to_player: bool,
    /// Update mob pathfinding when a block changes.
    pub update_pathfinding_on_block_update: bool,
}

impl Default for WorldMisc {
    fn default() -> Self {
        Self {
            allow_remote_ender_dragon_respawning: false,
            alternate_current_update_order: AlternateCurrentUpdateOrder::HorizontalFirstOutward,
            disable_end_credits: false,
            disable_relative_projectile_velocity: false,
            disable_sprint_interruption_on_attack: false,
            legacy_ender_pearl_behavior: false,
            max_leash_distance: MaybeInt::Default,
            redstone_implementation: RedstoneImplementation::Vanilla,
            show_sign_click_command_failure_msgs_to_player: false,
            update_pathfinding_on_block_update: true,
        }
    }
}

/// Redstone implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RedstoneImplementation {
    /// Vanilla redstone.
    Vanilla,
    /// Eigencraft redstone.
    Eigencraft,
    /// Alternate Current redstone.
    AlternateCurrent,
}

/// Block update order for the Alternate Current redstone implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AlternateCurrentUpdateOrder {
    /// Horizontal neighbors first, from the outside in.
    HorizontalFirstOutward,
    /// Horizontal neighbors first, from the inside out.
    HorizontalFirstInward,
    /// Vertical neighbors first, from the outside in.
    VerticalFirstOutward,
    /// Vertical neighbors first, from the inside out.
    VerticalFirstInward,
}

/// `scoreboards` (world).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WorldScoreboards {
    /// Treat non-player entities as never being on a team.
    pub allow_non_player_entities_on_scoreboards: bool,
    /// Use the vanilla scoreboard for nickname coloring.
    pub use_vanilla_world_scoreboard_name_coloring: bool,
}

impl Default for WorldScoreboards {
    fn default() -> Self {
        Self {
            allow_non_player_entities_on_scoreboards: true,
            use_vanilla_world_scoreboard_name_coloring: false,
        }
    }
}

/// `spawn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SpawnSettings {
    /// Allow signs inside spawn protection.
    pub allow_using_signs_inside_spawn_protection: bool,
    /// Keep spawn chunks loaded.
    ///
    /// UNVERIFIED: removed from the current Paper reference; kept for older files.
    pub keep_spawn_loaded: bool,
    /// Radius of chunks kept loaded around spawn.
    ///
    /// UNVERIFIED: removed from the current Paper reference; kept for older files.
    pub keep_spawn_loaded_range: i32,
}

impl Default for SpawnSettings {
    fn default() -> Self {
        Self {
            allow_using_signs_inside_spawn_protection: false,
            keep_spawn_loaded: true,
            keep_spawn_loaded_range: 10,
        }
    }
}

/// `tick-rates`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct TickRates {
    /// Per-entity behavior tick rates.
    pub behavior: BTreeMap<String, BTreeMap<String, i32>>,
    /// Container update rate.
    pub container_update: i32,
    /// Dry farmland tick rate.
    pub dry_farmland: i32,
    /// Grass spread rate.
    pub grass_spread: i32,
    /// Mob spawner tick rate; -1 disables spawners.
    pub mob_spawner: i32,
    /// Per-entity sensor tick rates.
    pub sensor: BTreeMap<String, BTreeMap<String, i32>>,
    /// Wet farmland tick rate.
    pub wet_farmland: i32,
}

impl Default for TickRates {
    fn default() -> Self {
        let mut behavior = BTreeMap::new();
        let mut villager = BTreeMap::new();
        villager.insert("validatenearbypoi".to_string(), -1);
        behavior.insert("villager".to_string(), villager);

        let mut sensor = BTreeMap::new();
        let mut villager_sensor = BTreeMap::new();
        villager_sensor.insert("secondarypoisensor".to_string(), 40);
        sensor.insert("villager".to_string(), villager_sensor);

        Self {
            behavior,
            container_update: 1,
            dry_farmland: 1,
            grass_spread: 1,
            mob_spawner: 1,
            sensor,
            wet_farmland: 1,
        }
    }
}

/// `unsupported-settings` (world).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WorldUnsupportedSettings {
    /// Stop ticking the world when no players or force-loaded chunks are present.
    pub disable_world_ticking_when_empty: bool,
    /// Fix the invulnerable end crystal exploit (MC-108513).
    pub fix_invulnerable_end_crystal_exploit: bool,
    /// Ticking toggles.
    pub ticking: WorldTicking,
}

impl Default for WorldUnsupportedSettings {
    fn default() -> Self {
        Self {
            disable_world_ticking_when_empty: false,
            fix_invulnerable_end_crystal_exploit: true,
            ticking: WorldTicking::default(),
        }
    }
}

/// `unsupported-settings.ticking`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct WorldTicking {
    /// Tick block entities.
    pub block_entities: bool,
    /// Tick chunks (mob spawning, random ticks, weather).
    pub chunks: bool,
}

impl Default for WorldTicking {
    fn default() -> Self {
        Self {
            block_entities: true,
            chunks: true,
        }
    }
}

const COMMENTS: &[KeyComment] = &[
    KeyComment {
        path: "_version_",
        lines: &["Paper world configuration version. Paper writes this as `_version_`."],
        unverified: false,
    },
    KeyComment {
        path: "anticheat",
        lines: &["Anti-cheat settings."],
        unverified: false,
    },
    KeyComment {
        path: "anticheat.anti-xray",
        lines: &["Anti-xray."],
        unverified: false,
    },
    KeyComment {
        path: "anticheat.anti-xray.enabled",
        lines: &["Enable anti-xray."],
        unverified: false,
    },
    KeyComment {
        path: "anticheat.anti-xray.engine-mode",
        lines: &["Engine mode 1, 2, or 3."],
        unverified: false,
    },
    KeyComment {
        path: "anticheat.anti-xray.hidden-blocks",
        lines: &["Blocks hidden from clients."],
        unverified: false,
    },
    KeyComment {
        path: "anticheat.anti-xray.replacement-blocks",
        lines: &["Blocks used as replacements in engine modes 1 and 2."],
        unverified: false,
    },
    KeyComment {
        path: "chunks",
        lines: &["Chunk loading and saving."],
        unverified: false,
    },
    KeyComment {
        path: "chunks.auto-save-interval",
        lines: &["Save interval in ticks; `default` uses ticks-per.autosave in bukkit.yml."],
        unverified: false,
    },
    KeyComment {
        path: "chunks.delay-chunk-unloads-by",
        lines: &["Delay chunk unloads by this duration, for example 10s."],
        unverified: false,
    },
    KeyComment {
        path: "chunks.entity-per-chunk-save-limit",
        lines: &["Per-entity save/load limit per chunk; -1 disables the limit."],
        unverified: false,
    },
    KeyComment {
        path: "chunks.max-auto-save-chunks-per-tick",
        lines: &["Maximum chunks auto-saved per tick."],
        unverified: false,
    },
    KeyComment {
        path: "chunks.prevent-moving-into-unloaded-chunks",
        lines: &["Prevent players from moving into unloaded chunks."],
        unverified: false,
    },
    KeyComment {
        path: "collisions",
        lines: &["Collision behavior."],
        unverified: false,
    },
    KeyComment {
        path: "entities",
        lines: &["Entity behavior and spawning."],
        unverified: false,
    },
    KeyComment {
        path: "entities.armor-stands",
        lines: &["Armor stand behavior."],
        unverified: false,
    },
    KeyComment {
        path: "entities.behavior",
        lines: &["Mob behavior tweaks."],
        unverified: false,
    },
    KeyComment {
        path: "entities.behavior.disable-player-crits",
        lines: &["Disable critical hits in PvP."],
        unverified: false,
    },
    KeyComment {
        path: "entities.mob-effects",
        lines: &["Mob effect immunities."],
        unverified: false,
    },
    KeyComment {
        path: "entities.mob-effects.immune-to-wither-effect",
        lines: &["Per-entity wither immunity."],
        unverified: false,
    },
    KeyComment {
        path: "entities.spawning",
        lines: &["Mob spawning."],
        unverified: false,
    },
    KeyComment {
        path: "entities.spawning.per-player-mob-spawns",
        lines: &["Count the mob cap per player."],
        unverified: false,
    },
    KeyComment {
        path: "entities.spawning.duplicate-uuid",
        lines: &["Duplicate UUID handling."],
        unverified: false,
    },
    KeyComment {
        path: "environment",
        lines: &["World environment behavior."],
        unverified: false,
    },
    KeyComment {
        path: "environment.frosted-ice",
        lines: &["Frosted ice behavior."],
        unverified: false,
    },
    KeyComment {
        path: "environment.nether-ceiling-void-damage-height",
        lines: &["Y level above which nether players take void damage; `disabled` turns it off."],
        unverified: false,
    },
    KeyComment {
        path: "environment.optimize-explosions",
        lines: &["Cache entity lookups during explosions."],
        unverified: false,
    },
    KeyComment {
        path: "environment.void-damage-amount",
        lines: &["Void damage per attempt; `disabled` turns void damage off."],
        unverified: false,
    },
    KeyComment {
        path: "fixes",
        lines: &["Bug fixes that change vanilla behavior."],
        unverified: false,
    },
    KeyComment {
        path: "hopper",
        lines: &["Hopper optimizations."],
        unverified: false,
    },
    KeyComment {
        path: "hopper.cooldown-when-full",
        lines: &["Apply a cooldown when a hopper is full."],
        unverified: false,
    },
    KeyComment {
        path: "hopper.disable-move-event",
        lines: &["Disable the InventoryMoveItemEvent for hoppers (breaks protection plugins)."],
        unverified: false,
    },
    KeyComment {
        path: "hopper.ignore-occluding-blocks",
        lines: &["Ignore containers inside occluding blocks."],
        unverified: false,
    },
    KeyComment {
        path: "maps",
        lines: &["Map behavior."],
        unverified: false,
    },
    KeyComment {
        path: "maps.item-frame-cursor-limit",
        lines: &["Maximum cursors per map."],
        unverified: false,
    },
    KeyComment {
        path: "maps.item-frame-cursor-update-interval",
        lines: &["Ticks between cursor updates; below 1 disables updates."],
        unverified: false,
    },
    KeyComment {
        path: "max-growth-height",
        lines: &["Maximum natural growth heights."],
        unverified: false,
    },
    KeyComment {
        path: "misc",
        lines: &["Assorted gameplay settings."],
        unverified: false,
    },
    KeyComment {
        path: "misc.redstone-implementation",
        lines: &["Redstone implementation: VANILLA, EIGENCRAFT, or ALTERNATE_CURRENT."],
        unverified: false,
    },
    KeyComment {
        path: "scoreboards",
        lines: &["Scoreboard behavior."],
        unverified: false,
    },
    KeyComment {
        path: "spawn",
        lines: &["Spawn behavior."],
        unverified: false,
    },
    KeyComment {
        path: "spawn.allow-using-signs-inside-spawn-protection",
        lines: &["Allow signs inside spawn protection."],
        unverified: false,
    },
    KeyComment {
        path: "spawn.keep-spawn-loaded",
        lines: &["Keep spawn chunks loaded."],
        unverified: true,
    },
    KeyComment {
        path: "spawn.keep-spawn-loaded-range",
        lines: &["Radius of chunks kept loaded around spawn."],
        unverified: true,
    },
    KeyComment {
        path: "tick-rates",
        lines: &["Tick rate overrides."],
        unverified: false,
    },
    KeyComment {
        path: "tick-rates.container-update",
        lines: &["Container update rate."],
        unverified: false,
    },
    KeyComment {
        path: "tick-rates.grass-spread",
        lines: &["Grass spread rate."],
        unverified: false,
    },
    KeyComment {
        path: "tick-rates.mob-spawner",
        lines: &["Mob spawner tick rate; -1 disables spawners."],
        unverified: false,
    },
    KeyComment {
        path: "unsupported-settings",
        lines: &["Settings Paper does not officially support."],
        unverified: false,
    },
    KeyComment {
        path: "unsupported-settings.disable-world-ticking-when-empty",
        lines: &["Stop ticking the world when no players or force-loaded chunks are present."],
        unverified: false,
    },
    KeyComment {
        path: "unsupported-settings.ticking",
        lines: &["Ticking toggles."],
        unverified: false,
    },
];

impl YamlConfig for WorldSettings {
    const KIND: ConfigFileKind = ConfigFileKind::PaperWorldDefaults;

    fn key_comments() -> &'static [KeyComment] {
        COMMENTS
    }
}
