# Agent sessions

Rule: sessions are long-lived. When a task finishes, send the next task to the same session
(`orca terminal send --terminal <handle> ...`); never close one and start a new one. Only if a
terminal dies, resume the same conversation (`codex resume <id>` / `freebuff --continue <id>`).
Limits: 1 Terra (Codex, `gpt-6.1-terra`) and up to 5 Freebuff sessions.

| Name | Agent | Worktree (Orca) | Terminal handle | Current task |
|---|---|---|---|---|
| Terra | codex `gpt-6.1-terra` | `C:/Users/taegy/orca/workspaces/ferrite-mc-ref/terra` | `term_dad9fe26-deed-4d13-a9c0-084b3176edd7` | T1: Folia ThreadedRegionizer port in ferrite-region |
| FB1 | freebuff | `.../ferrite-mc-ref/fb1` | `term_8649c83f-b7fb-4a23-ba35-be88532e993f` | salvage wip/day2: libdeflate + VarInt, protocol-translate |
| FB2 | freebuff | `.../ferrite-mc-ref/fb2` | `term_9ef08d09-6928-45a2-9d11-ab4801e9b34d` | salvage wip/day3: plugin registry/inventory (T3) |
| FB3 | freebuff | `.../ferrite-mc-ref/fb3` | `term_82d5f2a8-46a2-4716-b735-11c08cb2684f` | profiler into tick loop: /tps /mspt /profile |
| FB4 | freebuff | `.../ferrite-mc-ref/fb4` | `term_a16e214b-e198-48b8-a6e6-f58532e42cd7` | signed chat |
| FB5 | freebuff | `.../ferrite-mc-ref/fb5` | `term_21aab56d-55a9-40bc-a1b3-2466a4136177` | optimization-mod research → docs/OPTIMIZATIONS.md |

Handles are runtime-scoped: after an Orca restart, re-list with `orca terminal list --json`
and update this table.
