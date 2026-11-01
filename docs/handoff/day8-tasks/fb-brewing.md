# Freebuff task (brewing stand, beacon, remaining containers)

Base `63b5695`. Containers already on main: furnace/blast furnace/smoker, anvil, enchanting,
grindstone, stonecutter, smithing table, loom (oc7). Read `docs/handoff/day8-tasks/COMMON-RULES.md`.

Area: `crates/ferrite-server/src/game/` containers/menus, `ferrite-inventory`, `ferrite-item`,
`ferrite-recipe`, `ferrite-data` tables. Do not touch worldgen, pistons, command blocks, dimensions
or the plugin bridge.

Implement, in this order:

1. **Brewing stand**: block entity with 5 slots, blaze powder fuel (20 brews), 400-tick brew time,
   vanilla potion recipes (awkward -> effect potions, fermented spider eye inversions, gunpowder
   splash, dragon breath lingering, redstone extend, glowstone amplify), bottle filling from water,
   exact menu click validation, save/load through `/data merge block` and chunk persistence.
2. **Beacon**: 4-tier pyramid scan from `vanilla/26.3/generated/reports/blocks.json` tags
   (`beacon_base_blocks`), beam visibility, effect selection packet + range, regeneration/main-hand
   effects, 8-block range per level, payment with iron/gold/emerald/diamond/netherite ingot.
3. If both land: cartography table and shulker box (27 slots, no nesting, drops contents).

Tests per feature: brew timing and recipe output, fuel consumption, menu click rejection for an
invalid slot, beacon tier detection and effect application, beacon rejects a wrong base block.
