# Freebuff task (nether dimension and portals)

Base `63b5695`. Today `dimension` is fixed to the overworld (`game/mod.rs`, `game/region.rs`,
world storage). Read `docs/handoff/day8-tasks/COMMON-RULES.md`.

Area: `ferrite-server` dimension/world plumbing, `ferrite-world` storage paths, `ferrite-protocol`
`Respawn`/`ChangeDimension` packets if incomplete. Do not touch worldgen internals (the codex worker
owns `ferrite-worldgen`; call its existing entry points), pistons, command blocks or containers.

Scope, in order:

1. Dimension registry: `minecraft:overworld`, `minecraft:the_nether`, `minecraft:the_end` keys,
   per-dimension world directories (`<world>/DIM-1`, `DIM1`), per-dimension `ChunkSource`, spawn
   and sky/ceiling flags. The nether must generate real terrain from the existing vanilla generator
   settings (`vanilla/26.3` dimension data), not superflat.
2. Dimension transfer: `ChangeDimension` + `Respawn` sequence, position/rotation preserved, entity
   and player tracking removed/re-added, per-dimension player data, `/execute in` selectors resolve
   the right dimension.
3. Nether portals: obsidian frame ignition with flint and steel, `NetherPortalBlock` states, 8:1
   coordinate scaling, portal search (128-block radius in the target dimension), portal creation
   platform with the vanilla 2x3 minimum, 4-second wait + cooldown, return trip, portal sound.
4. The end is out of scope unless everything above is green with tests.

Tests: a player teleported to the nether keeps its inventory and position/8, a return portal brings
it back, a frame with the wrong shape does not light, portal search finds an existing portal before
creating one, the nether chunk source returns nether biomes.
