Common rules for the Day 7 batch. Base = `main` at `50ad5ed` (pushed). In YOUR worktree only: `git fetch origin`, then `git merge --no-ff origin/main` before you start. Read `AGENTS.md` and `docs/HANDOFF.md` first.

- Never guess ids: read `vanilla/26.3/generated/`. If `vanilla/` is missing in your worktree, copy it from the main checkout `C:/Users/taegy/OneDrive/Desktop/새 폴더/ferrite-mc-ref/vanilla` (it is gitignored).
- Work in commit-sized steps and commit after each green step. Do not touch files outside your task's area; another worker owns worldgen, pistons, command blocks, hoppers, villagers or persistence — see the task files.
- Disk is tight: `CARGO_INCREMENTAL=0`, `source tools/env.sh` in Git Bash, and `cargo clean` only inside your own worktree if free space drops below 10 GB.
- Acceptance per commit: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check` all green. If a timing-sensitive integration test fails only in the full suite, rerun it alone and report it; never weaken it.
- Do NOT push and do NOT merge into main; the coordinator merges and pushes.
- Finish with `worker_done` (commit hashes + test counts) or `outcome failed` naming exactly what blocks you. If you finish early, continue with the next item from `docs/BACKLOG.md` that does not conflict with another worker's area, in the same session, and report it too.
