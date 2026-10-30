# Agent sessions

Rule: sessions are long-lived. When a task finishes, send the next task to the same session
(`orca terminal send --terminal <handle> ...`); never close one and start a new one. Only if a
terminal dies, resume the same conversation (`codex resume <id>` / `freebuff --continue <id>`).
Limits: 1 Claude worker, 2 Terra (Codex, `gpt-6.1-terra`; Terra-2 runs at medium effort), up to 5 Freebuff and 2 opencode2 sessions.
Routing: the coordinator only orchestrates; hard tasks go to the Claude worker and Terra (as commit-sized
milestones), small well-specified tasks to Freebuff. Tasks are dispatched with `orca orchestration`.

| Name | Agent | Worktree (Orca) | Terminal handle | Current task |
|---|---|---|---|---|
| Claude worker | claude | `.../ferrite-mc-ref/claude-worker` | `term_bb3d8520-44ff-4c68-93eb-0ec559ee7b0a` | entity system: storage, tracking, item entities |
| Terra | codex `gpt-6.1-terra` | `C:/Users/taegy/orca/workspaces/ferrite-mc-ref/terra` | `term_dad9fe26-deed-4d13-a9c0-084b3176edd7` | ferrite-fluid (step 3: spreading) |
| Terra-2 | codex `gpt-6.1-terra` medium | `C:/Users/taegy/orca/workspaces/ferrite-mc-ref/terra2` | `term_a70e887c-6078-4f6c-b323-ba8599cf4583` | vanilla-exact terrain generation (density functions, noise fill, surface rules) |
| FB1 | freebuff | `.../ferrite-mc-ref/fb1` | `term_8649c83f-b7fb-4a23-ba35-be88532e993f` | salvage wip/day2: libdeflate + VarInt, protocol-translate |
| FB2 | freebuff | `.../ferrite-mc-ref/fb2` | `term_9ef08d09-6928-45a2-9d11-ab4801e9b34d` | salvage wip/day3: plugin registry/inventory (T3) |
| FB3 | freebuff | `.../ferrite-mc-ref/fb3` | `term_82d5f2a8-46a2-4716-b735-11c08cb2684f` | profiler into tick loop: /tps /mspt /profile |
| FB4 | freebuff | `.../ferrite-mc-ref/fb4` | `term_a16e214b-e198-48b8-a6e6-f58532e42cd7` | signed chat |
| FB5 | freebuff | `.../ferrite-mc-ref/fb5` | `term_21aab56d-55a9-40bc-a1b3-2466a4136177` | fast AES-CFB8 in ferrite-net (research doc merged) |
| OC1 | opencode2 | `.../ferrite-mc-ref/oc1` | `term_d0c78d1c-2373-452f-944f-c8879d85a6e2` | Velocity/BungeeCord forwarding |
| OC2 | opencode2 | `.../ferrite-mc-ref/oc2` | `term_3f4ae1f3-a757-44dd-ac4e-9c7a2eabc61f` | commands 1: gamemode/time/weather/... |

Handles are runtime-scoped: after an Orca restart, re-list with `orca terminal list --json`
and update this table.
