# Agent sessions

Rule: sessions are long-lived. When a task finishes, send the next task to the same session
(`orca terminal send --terminal <handle> ...`); never close one and start a new one. Only if a
terminal dies, resume the same conversation (`codex resume <id>` / `freebuff --continue <id>`).

**Day 5 limits: 1 Claude worker, 5 Freebuff.** Codex (`Terra`, `Terra-2`, `gpt-6.1-terra`) is out of
quota until 2026-10-31 00:00 — do not dispatch to it. The opencode2 sessions (OC1, OC2) are closed;
their work was merged on Day 4. Freebuff sessions end hourly and need a manual resume.

Routing: the coordinator only orchestrates; hard tasks go to the Claude worker (as commit-sized
milestones), small well-specified tasks to Freebuff. Tasks are dispatched with
`orca orchestration`.

Handles are runtime-scoped: after an Orca restart, re-list with `orca terminal list --json` and
update the table before sending anything.

| Name | Agent | Worktree (Orca) | Terminal handle | Current task |
|---|---|---|---|---|
| Claude worker | claude | `.../ferrite-mc-ref/claude-worker` | `term_bb3d8520-44ff-4c68-93eb-0ec559ee7b0a` | entities: store, tracker, item entities (merged); next: mob AI/spawning |
| FB1 | freebuff | `.../ferrite-mc-ref/fb1` | `term_8649c83f-b7fb-4a23-ba35-be88532e993f` | Day-5 task from the coordinator (writer task merged Day 4) |
| FB2 | freebuff | `.../ferrite-mc-ref/fb2` | `term_9ef08d09-6928-45a2-9d11-ab4801e9b34d` | Day-5 task from the coordinator (plugin event policy merged) |
| FB3 | freebuff | `.../ferrite-mc-ref/fb3` | `term_82d5f2a8-46a2-4716-b735-11c08cb2684f` | Day-5 task from the coordinator (commands 2 / selector gate merged) |
| FB4 | freebuff | `.../ferrite-mc-ref/fb4` | `term_788f444d-6209-41d8-9599-8615673d1284` | integration-test flake fix, selector dedup, HANDOFF/BACKLOG, release prep |
| FB5 | freebuff | `.../ferrite-mc-ref/fb5` | `term_21aab56d-55a9-40bc-a1b3-2466a4136177` | Day-5 task from the coordinator (clear/give/xp merged) |

Terra (`term_dad9fe26-deed-4d13-a9c0-084b3176edd7`) and Terra-2
(`term_a70e887c-6078-4f6c-b323-ba8599cf4583`): **out of quota until 2026-10-31 00:00**, no session
running. OC1 (`term_d0c78d1c-...)` and OC2 (`term_3f4ae1f3-...`) are closed; do not reuse them.

## Day 6 (2026-10-31) end state

All opencode quota is exhausted and Codex (Terra) is out of quota; Freebuff is resumed manually at
midnight. Full record, unmerged snapshot branches (`wip/day6-*`), the never-started tasks and the
orchestration procedure are in [`docs/handoff/day6-session-record.md`](handoff/day6-session-record.md);
the task texts are in `docs/handoff/day6-tasks/`.

| Session | Agent | Last task | State |
|---|---|---|---|
| FB1 | freebuff | translation layer, redstone core | merged to main |
| FB2 | freebuff | worldgen W2-W4, ores and vegetation | merged to main |
| FB3 / FB3b | freebuff | commands 3-5 (execute, data) | merged; `/function` finished by OC3 |
| FB4 | freebuff | playerdata fix, plugin bridge | merged; remainder on `wip/day6-oc4` |
| FB5 | freebuff | blocks, fire, explosions | merged to main |
| FB6 | freebuff | mobs | passive mobs merged; hostile mobs on `wip/day6-oc5` |
| FB7 | freebuff | survival, effects, potions | merged to main |
| OC3 | opencode2 | function command and datapack loader | done, on `wip/day6-oc3`, to merge |
| OC4 | opencode2 | plugin bridge takeover | WIP on `wip/day6-oc4` |
| OC5 | opencode2 | hostile mobs takeover | WIP on `wip/day6-oc5` |
| OC6, OC7, OC8 | opencode2 | worldgen throughput, containers, commands | never started |
