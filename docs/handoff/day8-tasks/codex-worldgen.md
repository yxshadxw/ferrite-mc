# Codex task (worldgen throughput + trees)

You own `ferrite-worldgen` and the server's world-generation path. No other worker touches them.

Read `AGENTS.md`, `docs/handoff/day6-tasks/oc6.md` and `docs/handoff/day7-tasks/oc6b.md`, then
`docs/handoff/day8-tasks/COMMON-RULES.md`.

Current state on `main` (`63b5695`): oc6b landed per-chunk density grids, memoized biome/surface
rules and a bounded worker pool. Measured **4.59 -> 6.18 chunks/s single-threaded** and
**23.49 -> 38.92 chunks/s at 8 threads**; the 10x target was NOT reached. The benchmark is
`cargo run --release -p ferrite-server --example genbench -- <threads> <chunks>`.

Goal, in order:

1. Reach **>= 10x** the single-thread baseline (>= 46 chunks/s at 8 threads, ideally 60+) with
   **byte-identical chunk output** versus `main`. Profile in release first and keep stage timings
   in every commit message. Likely wins: reusable scratch buffers instead of per-block `Vec`/
   `BTreeMap`/`String`, density-cell interpolation driven once per chunk, per-column biome/height
   caching, not oversubscribing the outer pool with `rayon` inside a chunk, moving lighting,
   heightmap and encoding off the region tick if they dominate.
2. Then trees: oak/birch/spruce trunk and foliage placers with the decoration RNG order of
   `ChunkGenerator.applyBiomeDecoration`, validated on the fixture chunks (report the match
   percentage of tree origins).

**You verify yourself — do not wait for the coordinator.** Every commit must keep
`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo fmt --all --check` green, and the `crates/ferrite-worldgen` Java differentials byte-identical.
Report the before/after numbers you actually measured, the exact command, and anything you could
not verify. Do not push or merge into `main`; the coordinator merges.
