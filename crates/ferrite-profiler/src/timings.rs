//! Hierarchical, per-region scoped timings.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

/// Region slot used by [`Timings::scope`] until a thread binds a region.
pub const GLOBAL_REGION: u32 = 0;

/// Number of region slots allocated by [`Timings::new`].
pub const DEFAULT_REGIONS: u32 = 16;

/// Maximum nesting depth recorded by [`Timings`].
///
/// Scopes entered deeper than this still accumulate calls and total time on
/// their node, but they are not pushed onto the thread stack, so their parent
/// link and the sampler see the scope one level up.
pub const MAX_DEPTH: usize = 64;

/// Direct-mapped size of the per-thread scope lookup cache.
const CACHE_SIZE: usize = 128;
const NO_PARENT: u32 = u32::MAX;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Records hierarchical named scopes, aggregated per region.
///
/// `scope` returns an RAII guard that measures its own lifetime:
///
/// ```
/// use ferrite_profiler::Timings;
///
/// let timings = Timings::new();
/// {
///     let _tick = timings.scope("tick");
///     {
///         let _entity = timings.scope("entity-tick");
///     }
/// }
///
/// let report = timings.report(0, 50.0);
/// assert_eq!(report.rows[0].path, "tick");
/// assert_eq!(report.rows[1].path, "tick/entity-tick");
/// ```
///
/// A scope's parent is whichever scope was open on the same thread when it was
/// entered, so the report mirrors the real call tree. Scope names, parent
/// links and region counters are created on first use and then cached in a
/// thread-local direct-mapped table, keeping the hot path to a hash, a table
/// probe and two clock reads. Nodes and names are arena-allocated for the
/// lifetime of the process (bounded by the number of distinct scope paths).
///
/// Region 0 is the global slot used by [`Timings::scope`] on threads that have
/// not called [`Timings::set_thread_region`]. Region threads bind their own
/// slot once and then record without passing a region id per call.
pub struct Timings {
    id: u64,
    regions: u32,
    registry: Mutex<Registry>,
    slots: Mutex<Vec<&'static ThreadSlot>>,
}

impl Timings {
    /// Creates timings with [`DEFAULT_REGIONS`] region slots.
    pub fn new() -> Self {
        Self::with_regions(DEFAULT_REGIONS)
    }

    /// Creates timings with room for `regions` region ids.
    ///
    /// # Panics
    ///
    /// Panics when `regions` is zero.
    pub fn with_regions(regions: u32) -> Self {
        assert!(regions > 0, "Timings needs at least one region slot");
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            regions,
            registry: Mutex::new(Registry::default()),
            slots: Mutex::new(Vec::new()),
        }
    }

    /// Number of region slots.
    pub fn regions(&self) -> u32 {
        self.regions
    }

    /// Enters a scope in the current thread's bound region.
    ///
    /// The returned guard records its lifetime when dropped. It is not `Send`:
    /// scopes belong to the thread that entered them.
    pub fn scope(&self, name: &str) -> ScopeGuard<'_> {
        let state = self.thread_state();
        let region = state.region.get();
        self.enter(name, state, region)
    }

    /// Enters a scope in an explicit region.
    ///
    /// Region ids outside `0..regions()` are recorded in [`GLOBAL_REGION`].
    pub fn scope_in(&self, region: u32, name: &str) -> ScopeGuard<'_> {
        let state = self.thread_state();
        let region = self.clamp_region(region);
        self.enter(name, state, region)
    }

    /// Binds the region used by [`Timings::scope`] on the calling thread.
    ///
    /// Out-of-range ids fall back to [`GLOBAL_REGION`].
    pub fn set_thread_region(&self, region: u32) {
        let state = self.thread_state();
        state.region.set(self.clamp_region(region));
    }

    /// Region counters currently bound to the calling thread.
    pub fn thread_region(&self) -> u32 {
        self.thread_state().region.get()
    }

    /// Builds the scope tree for one region.
    ///
    /// `tick_ms` is the reference tick length used for
    /// [`ReportRow::pct_of_tick`] (for example `50.0` at 20 TPS).
    /// Siblings are sorted by inclusive time, descending.
    pub fn report(&self, region: u32, tick_ms: f64) -> TimingsReport {
        self.build_report(Some(self.clamp_region(region)), tick_ms)
    }

    /// Builds the scope tree aggregated across every region.
    pub fn report_all(&self, tick_ms: f64) -> TimingsReport {
        self.build_report(None, tick_ms)
    }

    /// Zeroes every counter and every sampler sample count.
    ///
    /// Scope nodes and the per-thread lookup caches survive, so the next
    /// report starts a fresh period without re-interning names.
    pub fn reset(&self) {
        let registry = lock(&self.registry);
        for node in &registry.nodes {
            node.reset();
        }
    }

    fn clamp_region(&self, region: u32) -> u32 {
        if region < self.regions {
            region
        } else {
            GLOBAL_REGION
        }
    }

    fn enter<'a>(&'a self, name: &str, state: &'static ThreadState, region: u32) -> ScopeGuard<'a> {
        let depth = state.depth.get();
        let parent = if depth > 0 {
            state.frames[depth - 1].get()
        } else {
            None
        };
        let node = self.resolve(state, parent, name);
        node.counters(region).calls.fetch_add(1, Ordering::Relaxed);
        let start = Instant::now();
        let recorded = depth < MAX_DEPTH;
        if recorded {
            state.frames[depth].set(Some(node));
            state.depth.set(depth + 1);
            state.slot.push(node.index);
        }
        ScopeGuard {
            state,
            node,
            region,
            start,
            recorded,
            _timings: PhantomData,
        }
    }

    fn resolve(
        &self,
        state: &ThreadState,
        parent: Option<&'static Node>,
        name: &str,
    ) -> &'static Node {
        let parent_id = parent.map_or(NO_PARENT, |node| node.index);
        let hash = scope_hash(parent_id, name);
        let cache_index = hash as usize & (CACHE_SIZE - 1);
        let cached = state.cache[cache_index].get();
        if let Some(node) = cached.node
            && cached.hash == hash
            && node.name == name
            && node.parent.map_or(NO_PARENT, |node| node.index) == parent_id
        {
            return node;
        }

        let node = {
            let mut registry = lock(&self.registry);
            registry.resolve(parent, name, self.regions)
        };
        state.cache[cache_index].set(CacheSlot {
            hash,
            node: Some(node),
        });
        node
    }

    /// Returns (and lazily creates) this thread's state for this instance.
    fn thread_state(&self) -> &'static ThreadState {
        if let Some((owner, state)) = LAST.with(|last| last.get())
            && owner == self.id
        {
            return state;
        }

        let state = THREADS.with(|threads| {
            let mut threads = threads.borrow_mut();
            if let Some(thread) = threads.iter().find(|thread| thread.owner == self.id) {
                return thread.state;
            }
            let slot: &'static ThreadSlot = Box::leak(Box::new(ThreadSlot::new()));
            lock(&self.slots).push(slot);
            let state: &'static ThreadState = Box::leak(Box::new(ThreadState::new(slot)));
            threads.push(ThreadRef {
                owner: self.id,
                state,
            });
            state
        });
        LAST.with(|last| last.set(Some((self.id, state))));
        state
    }

    fn build_report(&self, region: Option<u32>, tick_ms: f64) -> TimingsReport {
        let nodes = self.node_snapshot();
        let mut active = vec![false; nodes.len()];
        let mut totals = vec![0u64; nodes.len()];
        let mut calls = vec![0u64; nodes.len()];

        for &node in &nodes {
            let (node_calls, node_total_ns) = match region {
                Some(region) => {
                    let counters = node.counters(region);
                    (
                        counters.calls.load(Ordering::Relaxed),
                        counters.total_ns.load(Ordering::Relaxed),
                    )
                }
                None => {
                    let mut node_calls = 0;
                    let mut node_total_ns = 0;
                    for counters in node.counters.iter() {
                        node_calls += counters.calls.load(Ordering::Relaxed);
                        node_total_ns += counters.total_ns.load(Ordering::Relaxed);
                    }
                    (node_calls, node_total_ns)
                }
            };
            let index = node.index as usize;
            active[index] = node_calls > 0;
            totals[index] = node_total_ns;
            calls[index] = node_calls;
        }

        let mut roots = Vec::new();
        let mut children: HashMap<u32, Vec<&'static Node>> = HashMap::new();
        for node in nodes {
            let index = node.index as usize;
            if !active[index] {
                continue;
            }
            match node.parent {
                Some(parent) if active[parent.index as usize] => {
                    children.entry(parent.index).or_default().push(node);
                }
                _ => roots.push(node),
            }
        }
        sort_nodes(&mut roots, &totals);

        let mut total_ms = 0.0;
        let mut context = ReportContext {
            children,
            totals,
            calls,
            tick_ms,
            rows: Vec::new(),
        };
        for root in roots {
            let root_total = context.totals[root.index as usize];
            total_ms += root_total as f64 / 1_000_000.0;
            context.visit(root, 0, root.name.to_owned(), root_total);
        }

        TimingsReport {
            region,
            tick_ms,
            total_ms,
            rows: context.rows,
        }
    }

    pub(crate) fn node_snapshot(&self) -> Vec<&'static Node> {
        lock(&self.registry).nodes.clone()
    }

    pub(crate) fn thread_count(&self) -> usize {
        lock(&self.slots).len()
    }

    pub(crate) fn lock_slots(&self) -> MutexGuard<'_, Vec<&'static ThreadSlot>> {
        lock(&self.slots)
    }
}

impl Default for Timings {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard returned by [`Timings::scope`].
///
/// Records the elapsed time of its lifetime into the scope node when dropped.
/// Guards must be dropped in reverse order of entry, which the borrow checker
/// enforces for ordinary lexical scopes.
pub struct ScopeGuard<'a> {
    state: &'static ThreadState,
    node: &'static Node,
    region: u32,
    start: Instant,
    recorded: bool,
    _timings: PhantomData<&'a Timings>,
}

impl Drop for ScopeGuard<'_> {
    fn drop(&mut self) {
        let elapsed_ns = self.start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        if self.recorded {
            let depth = self.state.depth.get();
            debug_assert!(depth > 0, "scope guard stack underflow");
            if depth > 0 {
                self.state.frames[depth - 1].set(None);
                self.state.depth.set(depth - 1);
                self.state.slot.pop();
            }
        }
        self.node
            .counters(self.region)
            .total_ns
            .fetch_add(elapsed_ns, Ordering::Relaxed);
    }
}

/// One row of a [`TimingsReport`] tree.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportRow {
    /// Nesting depth, 0 for a root scope.
    pub depth: usize,
    /// Scope label.
    pub name: String,
    /// Slash-separated path from the root, for example `tick/entity-tick/ai`.
    pub path: String,
    /// Number of times the scope was entered in this region and period.
    pub calls: u64,
    /// Inclusive time of the scope body, including nested scopes.
    pub total_ms: f64,
    /// Exclusive time: `total_ms` minus the inclusive time of direct children.
    pub self_ms: f64,
    /// `total_ms` as a percentage of the reference tick length.
    pub pct_of_tick: f64,
    /// `total_ms` as a percentage of the parent's inclusive time.
    pub pct_of_parent: f64,
}

/// Aggregated scope tree for one region, or for all regions.
#[derive(Debug, Clone, PartialEq)]
pub struct TimingsReport {
    /// Region reported, or `None` for the merged report.
    pub region: Option<u32>,
    /// Reference tick length used for [`ReportRow::pct_of_tick`].
    pub tick_ms: f64,
    /// Sum of the root scopes' inclusive time.
    pub total_ms: f64,
    /// Rows in depth-first order; siblings are sorted by `total_ms` descending.
    pub rows: Vec<ReportRow>,
}

impl fmt::Display for TimingsReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scope = match self.region {
            Some(region) => format!("region {region}"),
            None => "all regions".to_owned(),
        };
        writeln!(
            formatter,
            "Timings for {scope} (tick {:.2} ms, {:.3} ms scoped)",
            self.tick_ms, self.total_ms
        )?;
        for row in &self.rows {
            for _ in 0..row.depth {
                write!(formatter, "  ")?;
            }
            writeln!(
                formatter,
                "{:<40} {:>9.3} ms {:>5.1}% of tick {:>8} calls {:>9.3} ms self",
                row.path, row.total_ms, row.pct_of_tick, row.calls, row.self_ms
            )?;
        }
        Ok(())
    }
}

/// Mutable state the report walk needs.
struct ReportContext {
    children: HashMap<u32, Vec<&'static Node>>,
    totals: Vec<u64>,
    calls: Vec<u64>,
    tick_ms: f64,
    rows: Vec<ReportRow>,
}

impl ReportContext {
    fn visit(&mut self, node: &'static Node, depth: usize, path: String, parent_total: u64) {
        let index = node.index as usize;
        let total_ns = self.totals[index];
        let calls = self.calls[index];
        let mut children = self.children.remove(&node.index).unwrap_or_default();
        sort_nodes(&mut children, &self.totals);
        let children_ns: u64 = children
            .iter()
            .map(|child| self.totals[child.index as usize])
            .sum();
        let self_ns = total_ns.saturating_sub(children_ns);

        let total_ms = total_ns as f64 / 1_000_000.0;
        let self_ms = self_ns as f64 / 1_000_000.0;
        let pct_of_tick = if self.tick_ms > 0.0 {
            total_ms / self.tick_ms * 100.0
        } else {
            0.0
        };
        let pct_of_parent = if parent_total == 0 {
            100.0
        } else {
            total_ns as f64 / parent_total as f64 * 100.0
        };

        self.rows.push(ReportRow {
            depth,
            name: node.name.to_owned(),
            path: path.clone(),
            calls,
            total_ms,
            self_ms,
            pct_of_tick,
            pct_of_parent,
        });
        for child in children {
            let child_path = format!("{path}/{}", child.name);
            self.visit(child, depth + 1, child_path, total_ns);
        }
    }
}

fn sort_nodes(nodes: &mut [&'static Node], totals: &[u64]) {
    nodes.sort_by(|left, right| totals[right.index as usize].cmp(&totals[left.index as usize]));
}

/// Interned scope node. Never freed: nodes are arena-allocated for the process
/// lifetime so guards can hold `&'static` references without locks.
pub(crate) struct Node {
    pub(crate) index: u32,
    pub(crate) name: &'static str,
    pub(crate) parent: Option<&'static Node>,
    pub(crate) samples: AtomicU64,
    counters: Box<[Counters]>,
}

impl Node {
    fn counters(&self, region: u32) -> &Counters {
        &self.counters[region as usize]
    }

    pub(crate) fn add_sample(&self) {
        self.samples.fetch_add(1, Ordering::Relaxed);
    }

    fn reset(&self) {
        self.samples.store(0, Ordering::Relaxed);
        for counters in self.counters.iter() {
            counters.calls.store(0, Ordering::Relaxed);
            counters.total_ns.store(0, Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
struct Counters {
    calls: AtomicU64,
    total_ns: AtomicU64,
}

#[derive(Default)]
struct Registry {
    roots: HashMap<&'static str, &'static Node>,
    children: HashMap<u32, HashMap<&'static str, &'static Node>>,
    nodes: Vec<&'static Node>,
}

impl Registry {
    fn resolve(
        &mut self,
        parent: Option<&'static Node>,
        name: &str,
        regions: u32,
    ) -> &'static Node {
        let existing = match parent {
            None => self.roots.get(name).copied(),
            Some(parent) => self
                .children
                .get(&parent.index)
                .and_then(|children| children.get(name))
                .copied(),
        };
        if let Some(node) = existing {
            return node;
        }

        let interned: &'static str = Box::leak(name.to_owned().into_boxed_str());
        let node: &'static Node = Box::leak(Box::new(Node {
            index: self.nodes.len() as u32,
            name: interned,
            parent,
            samples: AtomicU64::new(0),
            counters: (0..regions).map(|_| Counters::default()).collect(),
        }));
        self.nodes.push(node);
        match parent {
            None => {
                self.roots.insert(interned, node);
            }
            Some(parent) => {
                self.children
                    .entry(parent.index)
                    .or_default()
                    .insert(interned, node);
            }
        }
        node
    }
}

/// Per-thread timing stack mirrored into an atomic slot for the sampler.
struct ThreadState {
    frames: [Cell<Option<&'static Node>>; MAX_DEPTH],
    depth: Cell<usize>,
    cache: Box<[Cell<CacheSlot>]>,
    slot: &'static ThreadSlot,
    region: Cell<u32>,
}

impl ThreadState {
    fn new(slot: &'static ThreadSlot) -> Self {
        Self {
            frames: std::array::from_fn(|_| Cell::new(None)),
            depth: Cell::new(0),
            cache: (0..CACHE_SIZE)
                .map(|_| Cell::new(CacheSlot::EMPTY))
                .collect(),
            slot,
            region: Cell::new(GLOBAL_REGION),
        }
    }
}

/// Atomic mirror of one thread's scope stack, readable by the sampler thread.
pub(crate) struct ThreadSlot {
    pub(crate) depth: AtomicU32,
    pub(crate) stack: [AtomicU32; MAX_DEPTH],
}

impl ThreadSlot {
    fn new() -> Self {
        Self {
            depth: AtomicU32::new(0),
            stack: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    fn push(&self, index: u32) {
        let depth = self.depth.load(Ordering::Relaxed);
        if (depth as usize) < MAX_DEPTH {
            self.stack[depth as usize].store(index, Ordering::Release);
            self.depth.store(depth + 1, Ordering::Release);
        }
    }

    fn pop(&self) {
        let depth = self.depth.load(Ordering::Relaxed);
        if depth > 0 {
            self.depth.store(depth - 1, Ordering::Release);
        }
    }
}

#[derive(Clone, Copy)]
struct CacheSlot {
    hash: u64,
    node: Option<&'static Node>,
}

impl CacheSlot {
    const EMPTY: Self = Self {
        hash: 0,
        node: None,
    };
}

struct ThreadRef {
    owner: u64,
    state: &'static ThreadState,
}

thread_local! {
    static THREADS: RefCell<Vec<ThreadRef>> = const { RefCell::new(Vec::new()) };
    static LAST: Cell<Option<(u64, &'static ThreadState)>> = const { Cell::new(None) };
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// FNV-1a over the parent id and scope name.
fn scope_hash(parent: u32, name: &str) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for byte in parent.to_le_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    for &byte in name.as_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    fn paths(report: &TimingsReport) -> Vec<&str> {
        report.rows.iter().map(|row| row.path.as_str()).collect()
    }

    #[test]
    fn report_builds_a_tree_sorted_by_total_time() {
        let timings = Timings::new();
        {
            let _tick = timings.scope("tick");
            thread::sleep(Duration::from_millis(5));
            {
                let _entity = timings.scope("entity-tick");
                thread::sleep(Duration::from_millis(20));
                {
                    let _ai = timings.scope("ai");
                    thread::sleep(Duration::from_millis(10));
                }
            }
            {
                let _chunk = timings.scope("chunk-io");
                thread::sleep(Duration::from_millis(2));
            }
        }

        let report = timings.report(GLOBAL_REGION, 50.0);
        assert_eq!(
            paths(&report),
            [
                "tick",
                "tick/entity-tick",
                "tick/entity-tick/ai",
                "tick/chunk-io"
            ]
        );
        assert_eq!(report.rows[0].depth, 0);
        assert_eq!(report.rows[1].depth, 1);
        assert_eq!(report.rows[2].depth, 2);
        assert_eq!(report.rows[3].depth, 1);

        let tick = &report.rows[0];
        assert_eq!(tick.calls, 1);
        assert!(tick.total_ms >= 37.0, "{tick:?}");
        assert!(tick.self_ms >= 5.0, "{tick:?}");
        assert!(tick.pct_of_tick >= 74.0, "{tick:?}");
        assert!(tick.self_ms < tick.total_ms);

        let entity = &report.rows[1];
        assert!(entity.total_ms >= 30.0, "{entity:?}");
        assert!(entity.self_ms >= 20.0, "{entity:?}");
        assert!(entity.pct_of_parent < 100.0, "{entity:?}");

        let ai = &report.rows[2];
        assert!(ai.total_ms >= 10.0, "{ai:?}");
        assert!(
            (ai.pct_of_parent - ai.total_ms / entity.total_ms * 100.0).abs() < 1e-9,
            "{ai:?}"
        );

        let chunk = &report.rows[3];
        assert!(chunk.total_ms >= 2.0, "{chunk:?}");
    }

    #[test]
    fn per_region_counters_are_isolated() {
        let timings = Timings::with_regions(4);
        {
            let _region_one = timings.scope_in(1, "tick");
            thread::sleep(Duration::from_millis(3));
        }
        {
            let _region_two = timings.scope_in(2, "tick");
            thread::sleep(Duration::from_millis(4));
        }

        let region_one = timings.report(1, 50.0);
        assert_eq!(paths(&region_one), ["tick"]);
        assert_eq!(region_one.rows[0].calls, 1);
        assert!(region_one.rows[0].total_ms >= 3.0);

        let region_two = timings.report(2, 50.0);
        assert_eq!(region_two.rows[0].calls, 1);
        assert!(region_two.rows[0].total_ms >= 4.0);

        assert!(timings.report(3, 50.0).rows.is_empty());

        let merged = timings.report_all(50.0);
        assert_eq!(merged.rows.len(), 1);
        assert_eq!(merged.rows[0].calls, 2);
        assert!(merged.rows[0].total_ms >= 7.0);
        assert!(merged.total_ms >= 7.0);
    }

    #[test]
    fn scopes_are_attributed_to_the_bound_region() {
        let timings = Timings::new();
        timings.set_thread_region(3);
        assert_eq!(timings.thread_region(), 3);
        {
            let _work = timings.scope("work");
        }

        assert_eq!(paths(&timings.report(3, 50.0)), ["work"]);
        assert!(timings.report(GLOBAL_REGION, 50.0).rows.is_empty());
    }

    #[test]
    fn same_name_nested_twice_creates_a_child() {
        let timings = Timings::new();
        {
            let _outer = timings.scope("a");
            {
                let _inner = timings.scope("a");
            }
        }
        let report = timings.report(GLOBAL_REGION, 50.0);
        assert_eq!(paths(&report), ["a", "a/a"]);
        assert_eq!(report.rows[0].calls, 1);
        assert_eq!(report.rows[1].calls, 1);
    }

    #[test]
    fn reset_starts_a_fresh_period() {
        let timings = Timings::new();
        {
            let _work = timings.scope("work");
        }
        assert_eq!(timings.report(GLOBAL_REGION, 50.0).rows.len(), 1);

        timings.reset();
        assert!(timings.report(GLOBAL_REGION, 50.0).rows.is_empty());

        {
            let _work = timings.scope("work");
        }
        let report = timings.report(GLOBAL_REGION, 50.0);
        assert_eq!(report.rows.len(), 1);
        assert_eq!(report.rows[0].calls, 1);
    }

    #[test]
    fn scope_percentages_of_tick_use_the_reference() {
        let timings = Timings::new();
        {
            let _half = timings.scope("half");
            thread::sleep(Duration::from_millis(10));
        }
        let report = timings.report(GLOBAL_REGION, 100.0);
        let row = &report.rows[0];
        assert!((row.pct_of_tick - row.total_ms).abs() < 1e-9, "{row:?}");
        assert!(row.pct_of_tick > 0.0, "{row:?}");
    }

    #[test]
    fn records_scopes_concurrently_from_eight_threads() {
        let timings = Arc::new(Timings::with_regions(8));
        let mut handles = Vec::new();
        for region in 0..8u32 {
            let timings = Arc::clone(&timings);
            handles.push(thread::spawn(move || {
                timings.set_thread_region(region);
                for _ in 0..1_000 {
                    let _work = timings.scope("work");
                }
            }));
        }
        for handle in handles {
            handle.join().expect("timing thread panicked");
        }

        let merged = timings.report_all(50.0);
        assert_eq!(merged.rows.len(), 1);
        assert_eq!(merged.rows[0].calls, 8_000);
        for region in 0..8u32 {
            let report = timings.report(region, 50.0);
            assert_eq!(report.rows.len(), 1, "region {region}");
            assert_eq!(report.rows[0].calls, 1_000, "region {region}");
        }
    }

    #[test]
    fn display_prints_the_tree() {
        let timings = Timings::new();
        {
            let _tick = timings.scope("tick");
            {
                let _entity = timings.scope("entity-tick");
            }
        }
        let text = timings.report(GLOBAL_REGION, 50.0).to_string();
        assert!(text.contains("tick"), "{text}");
        assert!(text.contains("tick/entity-tick"), "{text}");
        assert!(text.contains("% of tick"), "{text}");
    }
}
