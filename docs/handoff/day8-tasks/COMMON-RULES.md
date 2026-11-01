# Day 8 common rules

Base = `origin/main` at `63b5695` (already pushed). Work only in your own worktree:
`git fetch origin`, then `git merge --no-ff origin/main` before you start. Read `AGENTS.md` and
`docs/HANDOFF.md` first.

- Never guess ids: read `vanilla/26.3/generated/`. If `vanilla/` is missing in your worktree, copy it
  from `C:/Users/taegy/OneDrive/Desktop/새 폴더/ferrite-mc-ref/vanilla` (gitignored).
- Work in commit-sized steps; commit after each green step. Do not touch another worker's area:
  worldgen belongs to the codex worker (`oc6gpt`); pistons, command blocks, brewing/beacons,
  dimensions and the plugin API each belong to one freebuff worker.
- Disk is tight: `CARGO_INCREMENTAL=0`, `source tools/env.sh` in Git Bash. `cargo clean` only inside
  your own worktree if free space drops below 10 GB.
- Acceptance per commit: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all --check` all green. If a timing-sensitive integration test fails only under the
  full suite, rerun it alone and report it; never weaken it.
- Do NOT push and do NOT merge into main. The coordinator merges and pushes.
- Finish with `worker_done` (commit hashes + test counts) or `--outcome failed` naming exactly what
  blocks you. If you finish early, take the next non-conflicting item from `docs/BACKLOG.md` in the
  same session and report it too.
