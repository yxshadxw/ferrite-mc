# Freebuff task (pistons)

Implement `docs/handoff/day7-tasks/oc9.md` in full on top of `63b5695`; that file is the
specification and nothing about it has been started (the earlier oc9 session was stopped before
committing anything). Read `docs/handoff/day8-tasks/COMMON-RULES.md` first.

Area: `crates/ferrite-server/src/game/redstone.rs`, `game/blocks.rs`, `game/region.rs`, plus
`ferrite-world`/`ferrite-data` only for block states. Do not touch worldgen, command blocks,
containers or the plugin bridge.

Priority order if time runs short: state ids + extension/retraction timing, push rules and the
12-block limit with unmovable/block-entity refusal, sticky and slime/honey drag, entity displacement,
then tests. Every piston behaviour gets a test (lever extend, sticky retract, 13-block refusal,
chest refusal, slime drag, torch break, 100-cycle duplication check, 2-tick delay).
