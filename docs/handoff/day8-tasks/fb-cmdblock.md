# Freebuff task (command blocks)

Implement `docs/handoff/day7-tasks/oc13.md` in full on top of `63b5695`; that file is the
specification and nothing about it has been started (the earlier oc13 session was stopped before
committing anything). Read `docs/handoff/day8-tasks/COMMON-RULES.md` first.

Area: `ferrite-command` (packet + dispatcher context), `crates/ferrite-server/src/game/` block
entities, scheduled ticks, `commands_data.rs`, `commands_exec.rs`. Do not touch pistons (another
worker owns `game/redstone.rs` piston code; if you need the "command blocks are unmovable" rule,
add only the state check in the block-push table and say so), worldgen or containers.

Priority: block entity + NBT round-trip through `/data` and chunk save, impulse/chain/repeat
execution with the 1-tick double-execution guard, `/setblock` + `/setblock ... {Command:...}` and
the `SetCommandBlock` clientbound/`BlockEntityData` sync, then the safety cap and tests. Tests per
mode plus the conditional skip and the recursion cap.
