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
