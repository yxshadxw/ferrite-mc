# Day 6 session record (2026-10-31) — read this first when resuming

Written at the end of the Day 6 orchestration session because every opencode quota ran out and
Freebuff is resumed manually at midnight. It records what is on `main`, what exists only as a
snapshot branch, what was assigned but never started, and how the orchestration was run.

## 1. State of `main`

- `origin/main` = `151eceb`, pushed. `cargo test --workspace`: **978 passed, 0 failed, 2 ignored**
  (exit 0), `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check`
  clean. The repo is **public** (`yxshadxw/ferrite-mc`, GPL-3.0-or-later); no Mojang data and no
  secrets are tracked (`vanilla/` is gitignored and CI fails if anything under it is tracked).
- 128 commits since the Day 4 base `ac810a9`.

What reached `main` today, by area (worker that built it in brackets):

| Area | Content |
|---|---|
| Server list | live online count + 12-player sample, `server-icon.png` favicon, `enforcesSecureChat`, read timeouts for handshake/status/login (FB4) |
| Networking | per-connection writer task: compression/encryption off the tick thread, queue overflow disconnects "Timed out" (FB1); multi-version translation crate `ferrite-translate` 26.3 -> 26.2 with `supported-versions = all|native` (FB1) |
| Regionizer | Terra's 11 regionizer commits ported (transient regions, merge radius, proptest) (FB1) |
| Commands (~70) | gamemode/time/weather/seed/list/say/me (OC2), tp/msg/kill/op/ban/whitelist/stop (FB3), `/gamerule` 59 rules with persistence (OC1), clear/give/xp (FB5), effect/enchant/title/tellraw/playsound/particle/worldborder/spawnpoint (FB3), scoreboard/bossbar/tag/team (FB3), `/execute` and `/data` (FB3), setblock/fill/clone (FB1); selectors need permission level 2 (one dispatcher gate) |
| Inventory | `ferrite-inventory` menus, crafting grid and table, chests/barrels (Claude worker) |
| Survival | health, damage sources, hunger/saturation, eating, death and respawn with `keepInventory`, XP orbs, death XP (FB7) |
| Effects | all 32 mob effects, `/effect` store, potion contents codecs, drinking potions (FB7) |
| Blocks | neighbor updates + `canSurvive`, falling blocks, vanilla explosions/TNT (FB5), fire, doors/levers/buttons/beds/signs/buckets/cauldron, silk-touch drops (FB5), redstone core (FB1) |
| Mobs | mob framework, passive mobs (pig/cow/sheep/chicken), wander and look goals, natural spawning, `/summon` (FB6) |
| Plugins | plugin events (move, interact, break, place, command) with `plugins.on-event-error = allow|deny`, ~120/206 `Server` methods, minimal CraftBukkit shim, typical test plugin (FB2, FB1, FB4) |
| Worldgen | density functions, noise chunks, aquifers, surface rules, carvers matching the Java reference; server generates vanilla terrain on a worldgen thread (`generator = vanilla`, the default); ores and ground vegetation (Terra-2 -> FB2) |
| Tests | player data is scoped per world (fixes the shared-playerdata flake that made some integration tests hang) (FB4) |
| Docs | README, `docs/BACKLOG.md`, `docs/HANDOFF.md` Day 5 section, CI data guard |

## 2. Work that is NOT on `main` (snapshot branches on GitHub)

Worktrees live under `C:/Users/taegy/orca/workspaces/ferrite-mc-ref/<name>` and every one of them
was snapshotted to GitHub as `wip/day6-<name>` (a commit on top of the worktree branch holding its
uncommitted files; **may not compile**). Local `ferrite.toml` files are personal config and were
never included.

| Branch | Content | State / how to finish |
|---|---|---|
| `wip/day6-oc3` | `/function` command and datapack function loader (`game/functions.rs`, load/tick tags, macros, `return`, bot test `tests/tick_function.rs`), 3 commits | **Essentially done and committed.** Merge into `main`, run the full suite, push. (Taken over from FB3b) |
| `wip/day6-oc4` | plugin bridge: Bukkit scheduler tasks on the region scheduler, world/player snapshots, `Native.playerInventory` hook (8 files, ~150 lines) | WIP. Make Rust + Java build (`JAVA_HOME=C:/Program Files/Eclipse Adoptium/jdk-25.0.5.8-hotspot`), get `tests/plugin_lifecycle.rs` green. (Taken over from FB4) |
| `wip/day6-oc5` | hostile mobs and combat: `ferrite-entity` `path.rs` (pathfinding), `mob.rs`, `spawn.rs`, arrows, combat packets, region/integration changes (14 files, ~2000 lines) | WIP. Port is already on current main (base `3f0c168`). Needs build, then the bot tests listed in `day6-tasks/oc5.md`. (Taken over from FB6) |
| `wip/day6-fb3b`, `wip/day6-fb4`, `wip/day6-fb6` | the original uncommitted trees OC3/OC4/OC5 started from | Reference only; the oc* branches supersede them |
| `wip/day6-terra` | Terra's 12 regionizer commits | 11 already ported to main by FB1; the 12th (persistent gamerules) is superseded. Nothing to merge |
| `wip/day6-terra2` | Terra-2 worldgen WIP (`surface.rs`, features) | Superseded by FB2's merged W2/W3/W4 work; reference only |

## 3. Assigned but never started (opencode quota ran out)

Full instructions are saved in `docs/handoff/day6-tasks/` (plus `COMMON-RULES.md`, the acceptance
and base-branch rules every task assumes). Use them as the task text for Freebuff:

- `oc6.md` **worldgen throughput + trees** — highest priority. The 200-bot benchmark encoded only
  **69 chunks in 30 s** with the vanilla generator, so chunk loading is slow in real play. Needs a
  bounded parallel worker pool, density-function cache reuse, >= 10x chunks/s with byte-identical
  output, then tree features validated with a Java harness.
- `oc7.md` **containers**: furnace/blast furnace/smoker smelting, anvil, enchanting, grindstone,
  stonecutter, smithing, loom, drag/double-click/number-key click modes, shulker boxes.
- `oc8.md` **remaining commands**: `locate`, non-player `kill`/`tp`, `reload`, `datapack`, `help`,
  `schedule`, `trigger`, selector options (do not touch `/function`, OC3 did it).
- Also unstarted: pistons (redstone follow-up, FB1), the rest of the worldgen features and
  structures, villagers/trading, nether and end dimensions, entity persistence (see
  `docs/BACKLOG.md`).

Earlier task texts of the Freebuff sessions are kept for reference: `fb1c.md` (redstone),
`fb2c.md` (worldgen W4c), `fb3c.md` (commands 5 remainder), `fb4c.md` (merge and plugin bridge),
`fb5n.md` (fire), `fb6n.md` (hostile mobs), `fb7n.md` (effects).

## 4. Known issues

1. **Default `generator = vanilla` is slow** (see throughput above). `generator = superflat` is
   the fast fallback for tests and benchmarks; the user wants vanilla to stay the default because
   the goal is a real survival world, so fix throughput instead of reverting the default.
2. **Translation tests need `vanilla/26.2`** (gitignored, absent in CI): without it `build.rs`
   emits identity tables and the five 26.2 checks skip themselves. Fetch with
   `python tools/fetch_vanilla.py 26.2` for the real run. `build.rs` now watches the file so a later
   fetch triggers a rebuild.
3. **Timing-sensitive integration tests**: `survival_break_drops_an_item_that_falls_and_is_picked_up`
   and `chest_contents_are_shared_saved_and_spilled_when_broken` failed under heavy machine load
   in a few runs and pass alone; the root cause found was the shared playerdata directory (fixed).
   If they flake again, rerun alone before changing them; do not weaken the assertions.
4. **Disk**: each worktree builds its own `target/` (10+ GB). Delete `target/` of finished
   worktrees, build with `CARGO_INCREMENTAL=0`, and keep >= 20 GB free; builds crash at 0 free.
5. Plugin API breadth is partial (Server ~120/206); EssentialsX-style plugins do not fully work.
6. **Never tested with a real 26.3 client** — still the most valuable manual check.

## 5. How the orchestration was run (so it can be resumed)

- Coordinator: one Claude Code session in the main checkout, bound to the orchestration run
  `run_57911c7b3e38` (`orca orchestration run-use --id run_57911c7b3e38` after a restart; a
  `consumer_fenced` error means it must rebind). The coordinator only dispatches, merges, tests,
  pushes and keeps the README current; it does not implement features.
- Start a worker in a new worktree:
  `orca orchestration worker-start --run <run> --task <task_id> --agent freebuff|opencode2 --worktree new-top-level --name <name> --repo name:ferrite-mc-ref`
  (create the task first with `orca orchestration task-create --spec <text>`). Reuse an existing
  session with `--terminal <handle> --worktree path:<dir>`; never dispatch to a session that is
  still `working...` (the dispatch fails with a timeout). `opencode` is disabled, the id is `opencode2`.
- After a dispatch the long spec often stays as an unsubmitted paste chip (`N chars`) in the
  Freebuff input box: press Enter, wait a few seconds, press Enter again, then read the terminal
  until `working...` shows. A frozen Freebuff session (timer ran out) resumes when any text is
  sent followed by Enter and one more Enter. Closing a Freebuff tab in Orca can delete its
  worktree and branch.
- Merge routine used all day: merge the worker branch into local `main` (`--no-ff`), resolve
  conflicts keeping both sides, `cargo check --workspace --all-targets`, run the full suite,
  clippy and fmt, then push. Typical conflict spots are the `Player { .. }` constructor line in
  `game/mod.rs`, the `Command` enum and tick function in `game/region.rs`, and
  `tests/plugin_lifecycle.rs`.
- Keep the README current: it is updated in the same push that publishes a feature, so it never
  claims something that is not on GitHub.

## 6. Resume checklist (midnight, Freebuff)

1. `git fetch origin && git checkout main && git pull`; read this file and `docs/BACKLOG.md`.
2. Fetch vanilla data if missing: `python tools/fetch_vanilla.py` (26.3) and `... 26.2`.
3. Finish and merge `wip/day6-oc3` (done work), then `wip/day6-oc4` and `wip/day6-oc5`.
4. Start the unstarted tasks in this order: `oc6.md` (throughput), `oc8.md`, `oc7.md`.
5. After every merge: full suite, clippy, fmt, push, README update.
