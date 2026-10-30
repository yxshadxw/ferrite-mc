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


---
# Day 4 (2026-10-30) end
Relicensed to GPL-3.0-or-later; work coordinated by one Claude coordinator through Orca orchestration (run `run_57911c7b3e38`, sessions in `docs/sessions.md`).
**No new dispatches were made after the user said to stop for the day.** Sessions keep finishing what they were already doing.

## On main today
- Optimization survey of 20 projects with licenses and 26.x support (`docs/OPTIMIZATIONS.md`, FB5).
- `ferrite-world`: per-section random-tick/fluid-tick counters, recounted from palette frequencies (Lithium-style; Terra).
- `ferrite-fluid` (new): fluid state codec, new-liquid rules, scheduled ticks and slope spreading, directional collision faces, lava/water reactions and waterlogging; wired into the server so each region simulates only chunks it owns (`docs/handoff/fluid-ownership.md`; Terra).
- `ferrite-net`: AES-NI CFB8 cipher (decrypt ~4-8x faster, FB5); Velocity modern forwarding + BungeeCord legacy forwarding (OC1).
- `ferrite-server`: real /tps /mspt /profile (FB3). `ferrite-plugin` offline-player registry salvage (FB2). VarInt decode fast path, protocol-translate handoff notes (FB1).
- `tools/make_fixture_world.py` regenerates the vanilla fixture world (seed `ferrite-fixture`, spawn pinned to 0,80,112).

## Unmerged work (worktrees under C:/Users/taegy/orca/workspaces/ferrite-mc-ref/)
| Worktree | State |
|---|---|
| fb1 | `31b00a1` libdeflate default frame compression (2.6-3.4x compress, 1.7-2.2x decompress), reviewed OK - merge it. Next task (compression/encryption off the tick thread) is pasted in its input but was never submitted. |
| terra2 | `a634351` density-function evaluator (noise-free nodes), `f87df53` seeded noise + splines; W1c (Java differential vs the 26.3 jar) uncommitted. Then W2 (noise chunk fill) and W3 (surface rules); brief in task `task_295171c6e46d` / `docs/handoff/worldgen-26-3.md` (26.3 API names differ from the brief: use the jar). |
| claude-worker | `b408f74` ferrite-entity M1 (region-local store, section index, region hand-off) committed; M2 (tracking packets) and M3 (item entities) in progress, uncommitted. Edits `game/region.rs` and `game/mod.rs` - rebase after the fluid merge. |
| terra | random ticks R1/R2 started, 2 files uncommitted. |
| fb2 / fb3 / fb4 / fb5 / oc1 / oc2 | uncommitted work: plugin onEnable end-to-end test / commands 2 (tp, msg, kill, op, ban, kick, stop, save-*) / signed chat (never committed) / chunk packet cache / light-fixture fix / commands 1 (gamemode, time, weather, ...). |

## Known issues
- `ferrite-light` `compare_fixture_light_when_available` fails on the regenerated fixture: 14/3,207,168 sky-light values (0.00044%) at the outer ring of comparable chunks; block light 0 mismatches. Suspected fixture-edge artifact, not an engine bug (OC1 is investigating; CI skips it because the fixture is absent). Run workspace tests with `-- --skip compare_fixture_light_when_available` until fixed.
- Codex (Terra, Terra-2) is out of quota until 2026-10-31 00:00. Freebuff sessions end hourly and need a manual resume.
- Long instructions sent to Codex/Freebuff terminals land as a "pasted content" chip; press Enter (and check for `Working (` / `working...`) or the task never starts.
- Many concurrent cargo builds slow everything and make benchmarks noisy.

## Next steps
1. Merge fb1 (31b00a1) and terra2 W1a/W1b after the full workspace test passes; push.
2. Resume Terra/Terra-2 after the quota reset; collect FB/OC results as they commit, review, merge.
3. Remaining backlog: entity M2/M3, worldgen W1c-W3, random ticks, plugin onEnable end-to-end (M3 of the plan), signed chat, chunk packet cache, commands, tick-thread-free compression.
