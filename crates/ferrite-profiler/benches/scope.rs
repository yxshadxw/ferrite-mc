//! Measures the cost of entering and leaving a timing scope.
//!
//! Run with `cargo bench -p ferrite-profiler`. The number to look for is the
//! time per iteration of `scope_enter_exit_cached`, the steady-state path a
//! tick handler pays after the first entry of a scope.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use ferrite_profiler::Timings;

fn scope_cost(criterion: &mut Criterion) {
    let timings = Timings::new();
    // The first entry interns the name and fills the thread-local lookup
    // cache, so the benchmark only measures the hot path.
    drop(timings.scope("leaf"));

    let mut group = criterion.benchmark_group("timings");

    group.bench_function("scope_enter_exit_cached", |bencher| {
        bencher.iter(|| {
            let guard = timings.scope(black_box("leaf"));
            black_box(&guard);
        });
    });

    group.bench_function("scope_enter_exit_nested_3", |bencher| {
        bencher.iter(|| {
            let outer = timings.scope(black_box("tick"));
            let middle = timings.scope(black_box("entity-tick"));
            let leaf = timings.scope(black_box("leaf"));
            black_box((&outer, &middle, &leaf));
        });
    });

    group.finish();
}

criterion_group!(benches, scope_cost);
criterion_main!(benches);
