# Handoff — end of 2026-10-28 session

Branch `phase1-foundation` (HEAD `c8989a7`): workspace green — 131 tests, clippy -D warnings, fmt.
Plan: `C:\Users\taegy\.claude\plans\minecraft-lexical-dijkstra.md` (9 phases). Target MC 26.3 / protocol 777.

## Done (committed)
| Crate | State |
|---|---|
| ferrite-protocol | codec, framing+compression, status/login/configuration/play packets |
| ferrite-net | offline+online login (RSA/AES-CFB8, Mojang hasJoined), known-pack registry+tag sync, typed play channel |
| ferrite-server | config (ferrite.toml), game loop on ferrite-region: movement, chunk streaming (superflat), player visibility, chat, Ctrl+C |
| ferrite-region | Folia-style regionizer v2 (region-local safe points, per-region queues, heap deadlines) |
| ferrite-light | Starlight-style engine, **0 mismatches vs vanilla** on 256 chunks; 8.4 ms/chunk, 0.066 ms/torch |
| ferrite-world | paletted containers, chunk columns, heightmaps, Anvil .mca/.mcc, 26.3 chunk NBT, network encoding |
| ferrite-nbt | binary IO, SNBT, serde feature |
| ferrite-data | static block-state / registry / packet-ID tables from vanilla reports |
| ferrite-plugin-meta | plugin.yml / paper-plugin.yml, Paper-compatible load order, ThreadingMode |
| ferrite-jvm | lazily embedded JVM (Java 25), natives, exception mapping |

Measured: release server idle RSS **16.7 MB**, status ping 17.5 ms.
**Never tested with a real 26.3 client yet** — do this first (`cargo run --release -p ferrite-server`
with `ferrite.toml`: `online-mode = false`), then join `localhost`.

## Unverified WIP — branch `wip/freebuff-2026-10-28` (`f70e1dd`)
Interrupted, never built. Too big for freebuff; reassign to Codex terra, starting from this code:
- **F5** tools/extractor + ferrite-data `block_props` (exact light/collision/heightmap props from the unobfuscated jar). Unblocks exact heightmaps (world) and voxel-shape light occlusion (light).
- **F6** java/ferrite-api-impl (Paper API `Server` impl, plugin loader, schedulers).
- **F7** crates/ferrite-command (Brigadier-compatible dispatcher + Commands packet).

## Next steps (in order)
1. Real-client join test; fix whatever breaks.
2. Terra: finish F5 → wire into ferrite-world `BlockInfo` and ferrite-light `properties.rs` (exact face shapes); rerun light differential.
3. Terra: A3 leftovers — 200-bot/30 s load test (MSPT + RSS), review `game/mod.rs` duplicate-UUID + transfer races (see A3 notes below).
4. Terra: wire ferrite-world Anvil loading into ferrite-server's `ChunkSource` (load real worlds, save on unload).
5. Terra: F6 + ferrite-jvm → first real plugin `onEnable` inside Ferrite; then F7 commands into play.
6. Regionizer: move deferred mutations into per-region queues (currently a shared queue scanned).
7. Phase 2 remainder: block place/break, signed chat, inventory basics.

### Worker notes
- A3: validate `game/region.rs` `Player::update` / `Player::chunks_tick` and `game/global.rs` `GlobalRegion::tick` under sustained concurrency.
- B3: `ferrite-light/src/properties.rs` `classify` + `LightProperties::faces` approximate special shapes with quadrant masks.
- C2: see `docs/handoff/c2-world-storage.md` (fluids/heightmap/biome IDs in registry.rs, SNBT/serde edge cases, palette perf).

## Environment
- Rust: `source tools/env.sh` (cargo in ~/.cargo/bin), toolchain stable-x86_64-pc-windows-msvc; Windows SDK installed.
- Java 25: `C:/Program Files/Eclipse Adoptium/jdk-25.0.5.8-hotspot` (JAVA_HOME for `python tools/fetch_vanilla.py`).
- Vanilla data + fixture world + protocol docs (gitignored): `vanilla/26.3/`, `vanilla/docs/`.
- Orca run `run_4622c45f6259`; F5/F6/F7 dispatches were never settled (interrupted) — create new tasks rather than reusing them.

---
# Day 2 (2026-10-28) end
phase1-foundation HEAD `0eac5a9`. Committed today: exact block props, fluid count fix, palette/light perf,
real-world Anvil loading + persistence, 200-bot load test (avg 0.4 ms MSPT), regionizer v3, ferrite-command,
ferrite-text, ferrite-math, ferrite-console, ferrite-noise (bit-exact), SNBT hardening, plugin bridge checkpoint.
NOT re-verified by coordinator at the end (usage limit): the last two commits rely on worker-reported checks — run tools/check.sh first.
Unverified WIP on branch `wip/day2-2026-10-28`: T2 block place/break/inventory/commands (creative-break test still red;
ops-by-UUID security fix done), ferrite-recipe (no vanilla tests yet), ferrite-protocol-translate (agent hit rate limit mid-pair-module),
FB11 libdeflate/VarInt (clippy collapsible_if at codec.rs:98), FB12 profiler / FB13 item never started (prompts unsent).
Also: delete crates/ferrite-noise/logs/ (accidentally committed log). T3 remaining: java/T3-CHECKPOINT.md. FB15 part 2 (ferrite-data tags) not started.


---
# Day 3 (2026-10-29) end
Branch phase1-foundation. **A real 26.3 client now joins, walks, breaks/places blocks** (offline mode, superflat). Fixed along the way:
3 missing synchronized registries, light BitSet wire format, plus chunk streaming (prefetch) and player resume (playerdata/<uuid>.json).
`python tools/wirecheck/run.py` (server must be running) verifies every packet against the real client codecs — run it after ANY packet change.

Committed today: exact block props, fluid-count/palette/light perf, real-world loading + persistence, phase-2 gameplay (break/place/commands/ops by UUID),
loot tables, recipes (1,763), item components + wire codec, entity data tables, tags, worldgen data model + multi-noise biome source,
admin lists, config files (server.properties/bukkit/spigot/paper), profiler crate, console crate, admin/config crates, plugin bridge (Paper-only loading, RegionContext).
Parked on `wip/day3-2026-10-29`: T3's unfinished registry/inventory work (does NOT compile: OfflinePlayerSnapshot / register_offline_player missing). Earlier: `wip/day2-2026-10-28` (translate crate).
Running freebuff tasks at end of day (prompts may be unsent in their tabs): FB21 ferrite-fluid, FB22 ferrite-explosion, FB23 ferrite-rules — check `git status` for their crates, verify, commit.

## Goals (user)
ALL vanilla features incl. every command; Rust-first, Java only when unavoidable. **Tomorrow: use Codex (one terra at a time) and PUBLISH TO GITHUB.**

## Next steps (priority)
1. Wire `ferrite-profiler` into the tick loop: /tps /mspt (currently prints "TPS unavailable"); then `/spark`-like profile.
2. Commands: all vanilla commands (commands.json has the tree; ferrite-command loads it; most nodes are stubs "Command not implemented") — implement by family: gamemode/tp/give/clear/effect/xp/time/weather/difficulty/gamerule/kill/summon/setblock/fill/clone/data/execute/scoreboard/team/bossbar/title/tellraw/say/me/msg/list/kick/ban/op/whitelist/save-all/stop/... 
3. Signed chat (the "Chat message validation failed" notice appears because chat is relayed unsigned) — implement secure chat or disguised chat properly.
4. Block-change fidelity: neighbor updates/placement rules (FB23), fluids (FB21), explosions (FB22), redstone, random ticks, gravity blocks.
5. Entities & mobs (largest): entity ECS, metadata/tracking, AI goals, spawning, combat, drops (ferrite-loot ready), item entities; inventory clicks (hashed slots) & crafting (ferrite-recipe ready).
6. World generation: density-function evaluator on ferrite-noise + biome_source + surface rules + features/structures (phase 6).
7. Multi-version translation layer (wip/day2 has the start).
8. Plugin API breadth (Server 26/206 implemented), EssentialsX needs CraftBukkit shim; compat-report.md on the WIP branch has probe results.
9. **GitHub release prep**: LICENSE-MIT/LICENSE-APACHE, README polish, CI check on Linux (rust-toolchain.toml pins MSVC — CI overrides), confirm no vanilla/ or Mojang data committed (`git ls-files | grep -i vanilla`), choose repo name, push.
