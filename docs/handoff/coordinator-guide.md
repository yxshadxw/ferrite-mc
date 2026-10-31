# Coordinator guide (handover from the Claude coordinator to a Freebuff coordinator)

You are the **coordinator** of the ferrite-mc project. You do not implement features. You start
workers, read their reports, merge their branches into `main`, run the checks, push, and keep
the README current. Read `docs/handoff/day6-session-record.md` first (state of the project) and
`docs/BACKLOG.md` (remaining work). Work in the main checkout
`C:\Users\taegy\OneDrive\Desktop\새 폴더\ferrite-mc-ref` (branch `main`). Never edit files inside
a worker's worktree.

## Standing rules from the user (all pre-authorized, do not ask)
- Merge verified worker branches into `main`, commit, and **push right away** (`git push origin main`,
  never force-push). Report failures honestly; if a merge needs real design work, send it back to
  the worker that owns the branch.
- Keep `README.md` current in the same push that publishes a feature, and never claim something
  that is not on GitHub.
- The default world generator stays `vanilla` (the goal is a real survival world). Fix chunk
  generation throughput instead of switching to superflat.
- No Terra/Codex tasks (its quota is gone), no opencode tasks (quota gone, the NVIDIA free API
  returned HTTP 429 with several sessions). Freebuff and the Claude worker are the only agents.
- Deleting `target/` of finished worktrees is allowed when disk is low (keep >= 20 GB free).
  Never delete worktrees, branches, or anything in the main checkout. Do not close a Freebuff tab in
  Orca: it can delete the worktree and branch.
- Never store API keys anywhere (repo, memory, docs). One NVIDIA key was pasted in chat: it should
  be revoked by the user.

## Orchestration basics
- The run is `run_57911c7b3e38`. After a restart bind with
  `orca orchestration run-use --id run_57911c7b3e38` (error `consumer_fenced` means rebind).
- Read messages: `orca orchestration check --run run_57911c7b3e38 --types worker_done,escalation,question --wait --timeout-ms 540000 --json`
  and acknowledge with `--ack <deliveryId>` on the next call. Heartbeats are only liveness.
- List workers and their terminal handles: `orca orchestration worker-list --run <run> --json`.
- Create a task: `orca orchestration task-create --run <run> --task-title <t> --spec <text> --json`.
- Start a worker on an existing worktree:
  `orca orchestration worker-start --run <run> --task <id> --agent freebuff --worktree "path:C:/Users/taegy/orca/workspaces/ferrite-mc-ref/<name>"`.
  New worktree: `--worktree new-top-level --name <name> --repo name:ferrite-mc-ref`.
  Reuse a finished session: `--terminal <handle> --worktree path:<dir>` (never while it shows `working...`).
- Dead dispatch: `orca orchestration worker-abandon --dispatch <id>`.
- After a dispatch the long spec often stays as an unsubmitted paste chip (`N chars`) in the Freebuff
  input box. Read the terminal (`orca terminal read --terminal <h> --json`); if the chip is there press
  Enter (`orca terminal send --terminal <h> --enter`), wait a few seconds, press Enter again, then
  read until `working...` appears. A frozen Freebuff session resumes when you send any text plus
  Enter, then Enter once more. If the user says they pressed Enter themselves, stop sending Enter.
- Always pass `< /dev/null` to `orca` inside shell loops, otherwise it eats the loop input.
- Never put backticks inside an unquoted bash heredoc; use a quoted heredoc or a script file.

## Merge routine (used all day)
1. `git status` clean on `main`; `git fetch origin`.
2. `git merge --no-ff yxshadxw/<branch>`. Resolve conflicts keeping both sides. Typical hot spots:
   the `Player { .. }` constructor line in `crates/ferrite-server/src/game/mod.rs`, the `Command`
   enum and tick function in `game/region.rs`, `tests/plugin_lifecycle.rs`, `Cargo.lock`.
3. `cargo check --workspace --all-targets`, then `cargo test --workspace` (several minutes; run it
   in the background and read the log), `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo fmt --all --check`. Use `CARGO_INCREMENTAL=0`; `source tools/env.sh` first in Git Bash.
4. Commit the merge, update the README if a user-visible feature landed, `git push origin main`.
5. If a timing-sensitive integration test fails only in the full suite, rerun it alone and report it;
   never weaken it.

## Current assignments (2026-11-01)
Each task text is in `docs/handoff/day6-tasks/`. Snapshots of unfinished work are GitHub branches
`wip/day6b-*` (same commits as the local worktree branches `yxshadxw/oc4` ... `oc8`).

| Worktree | Branch | Task |
|---|---|---|
| oc4 | yxshadxw/oc4 | plugin bridge: Bukkit scheduler on the region scheduler, world/player snapshots, `Native.playerInventory` (`oc4.md`) |
| oc5 | yxshadxw/oc5 | hostile mobs and combat: pathfinding, zombie/skeleton/creeper, arrows, spawning (`oc5.md`) |
| oc6 | yxshadxw/oc6 | **world generation throughput** (>= 10x chunks/s, byte-identical output) then trees (`oc6.md`), highest priority |
| oc7 | yxshadxw/oc7 | containers: furnace, anvil, enchanting, stonecutter, ... (`oc7.md`) |
| oc8 | yxshadxw/oc8 | remaining commands: locate, entity kill/tp, reload, datapack, selector options (`oc8.md`) |

When a worker reports `worker_done`, merge it (routine above), push, then give that session the next
item from `docs/BACKLOG.md` (pistons, villagers and trading, nether/end dimensions, entity
persistence, more Paper API surface) in the same session. Update `docs/handoff/day6-session-record.md`
(or add a new dated record) when the state changes materially.

## Known issues to keep in mind
- vanilla generator is slow until oc6 lands (benchmark: 69 chunks in 30 s with 200 bots).
- `ferrite-translate` tests need `vanilla/26.2` (gitignored); without it they skip.
- Each worktree builds its own `target/`; free disk by deleting `target/` of finished worktrees.
- Never tested with a real 26.3 client.
