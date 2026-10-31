OC6b task (worldgen throughput, continuation). You work in the EXISTING worktree `C:/Users/taegy/orca/workspaces/ferrite-mc-ref/oc6`, branch `yxshadxw/oc6`, HEAD `53cbe72` ("Benchmark and parallelize chunk generation (WIP)"). Merge `main` into your branch first (`git fetch origin && git merge --no-ff origin/main`), then read `AGENTS.md` and `docs/handoff/day6-tasks/oc6.md`.

What already exists on the branch: a bounded worker pool around the terrain generator (each worker owns one `SurfaceGenerator`/`FeatureGenerator`, previously constructed per chunk), a small FNV cache for biome samples, and `crates/ferrite-server/examples/genbench.rs` (release benchmark: `cargo run --release -p ferrite-server --example genbench -- <threads> <chunks>`).

Measured on this machine (release, 20 logical cores): **4.91 chunks/s single-threaded** (203.6 ms/chunk wall), **26.67 chunks/s at 8 threads** (5.4x; ~297 ms CPU/chunk under contention). The worldgen-crate benchmark splits one chunk into noise fill ~93 ms and surface rules ~80 ms, so per-chunk work is the bottleneck, not the pool.

Goal: at least **10x** the single-thread server throughput (>= 49 chunks/s with 8+ threads) and a lower single-chunk cost, with **byte-identical chunk output vs main** (hash the chunk block data + biomes + heightmaps before and after in a test or the benchmark and compare; the Java differential tests in `crates/ferrite-worldgen` must stay green). Never change the config default away from `generator = vanilla`.

Work in this order, one verified commit per step:
1. Profile first (`--release`, record stage timings). Keep the benchmark numbers in the commit message.
2. Reduce per-column work: cache biome samples per column/quart, cache interpolated density cell corners across the whole chunk instead of recomputing per block (`evaluator.rs` already has `cached_cell`; make sure the chunk fill drives it), replace per-block `Vec`/`BTreeMap`/`String` work in `noise_chunk.rs`/`surface.rs` with reusable scratch buffers.
3. Parallelize inside a chunk where safe (noise fill columns are independent; surface rules are per column). Use the existing `rayon` dependency; do not oversubscribe the outer pool.
4. Only after 1-3, tune the pool (thread count, queue depth, batch size) and consider moving lighting/encoding off the region tick.
5. Re-measure with 1, 8 and `NUMBER_OF_PROCESSORS` threads and report chunks/s, ms/chunk and the CPU breakdown.

Acceptance per commit: `source tools/env.sh`, `CARGO_INCREMENTAL=0`, then `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check` all green. Do NOT push and do NOT merge into main; the coordinator merges. Finish with `worker_done` naming the before/after chunks/s, the thread counts, the hash comparison result and the commit hashes; use `outcome failed` naming exactly what blocks the 10x target if you cannot reach it.
