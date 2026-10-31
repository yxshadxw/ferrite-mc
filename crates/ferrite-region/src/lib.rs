//! Region ownership and independent ticking for chunk sections.
//!
//! Sections use floor division, including for negative chunk coordinates. Two
//! sections whose Chebyshev distance is at most the merge radius share a region.
//! Each held chunk belongs to exactly one section and each section to one region.
//! The scheduler temporarily takes region data out of the topology while ticking.
//! The sole metadata mutex is never held while executing ticks or tasks; there
//! are no other scheduler locks, so tick callbacks cannot form a lock cycle.
//! Tasks retain their chunk address and are rerouted on topology changes.
//!
//! # Topology mutation protocol
//!
//! A chunk add/remove *involves* the regions owning a section within the merge
//! radius of the chunk's section (the regions it may merge, split or grow).
//!
//! * **Immediate.** If every involved region is idle and unreserved, and no
//!   pending mutation lies within the merge radius, the mutation is applied at
//!   once under the metadata lock.
//! * **Deferred.** Otherwise it receives a sequence number and is recorded on
//!   each involved region's own queue (`Region::mutations`, ordered by sequence
//!   number) and in a section index used only for nearby-conflict lookups. A
//!   region with a non-empty queue is *reserved*: it finishes any in-flight tick
//!   but starts no new one, so it reaches its post-tick safe point promptly.
//! * **Apply.** A deferred mutation is applied once all its involved regions are
//!   idle, it heads every one of their queues, and no lower-numbered pending
//!   mutation lies within the merge radius. Checks are triggered only locally:
//!   when an involved region returns from its tick (its queue head is checked),
//!   when a queue's head changes, or when a nearby mutation is applied. Nothing
//!   scans a global list, and regions not involved are never paused.
//! * **Multi-region mutations** (e.g. a bridge merging several regions) reserve
//!   all their regions at once; idle ones simply wait for the ticking ones.
//!   Involved regions are recomputed right before applying, so regions renamed
//!   or created by earlier mutations are picked up (and reserved) correctly.
//!
//! Deadlock freedom: sequence numbers totally order pending mutations, so the
//! lowest-numbered one heads all its queues and has no earlier nearby conflict;
//! its regions are reserved and therefore become idle after their in-flight
//! ticks, at which point it applies. Starvation freedom: a mutation only ever
//! waits for lower-numbered mutations (finitely many) and for in-flight ticks;
//! newer mutations queue behind it. Mutations on the same chunk, and nearby
//! mutations in general, apply in submission order.
//!
//! # Scheduling and fairness
//!
//! Workers pick the region with the earliest deadline. An overloaded region's
//! lag is capped at [`SchedulerConfig::max_catchup`] periods (excess periods are
//! dropped and counted in [`TickStats::skipped_ticks`]), so it never bursts
//! after load drops; under sustained overload this degrades to round-robin by
//! last dispatch. Split pieces inherit the parent's deadline, and a merged
//! region takes the section-weighted mean of its parts' deadlines, which
//! conserves lag, so merging gives no priority advantage. Fairness is per
//! *region*: a merged region ticks all its sections as one unit, at the same
//! rate as any other region but with proportionally longer ticks. When workers
//! are saturated, sections that keep merging and splitting therefore see their
//! TPS jitter (by about ±50% at 1 thread in `examples/throughput.rs`) even
//! though the long-run share is even. Sharing CPU in proportion to section
//! count would need cost-aware (weighted fair) scheduling, which is not done.

use parking_lot::{Condvar, Mutex};
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkPos(pub i32, pub i32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SectionPos(pub i32, pub i32);
pub type RegionId = u64;

/// Lifecycle states of Folia ThreadedRegionizer.ThreadedRegion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionState {
    Ready,
    Ticking,
    Transient,
    Dead,
}

/// World-owned state. `split` removes and returns data belonging to `sections`.
pub trait RegionData: Default + Send + 'static {
    fn merge(&mut self, other: Self);
    fn split(&mut self, sections: &HashSet<SectionPos>) -> Self;
}

struct Region<D> {
    state: RegionState,
    merge_targets: HashSet<RegionId>,
    merge_from: HashSet<RegionId>,
    sections: HashSet<SectionPos>,
    data: Option<D>,
    next_tick: Instant,
    stats: TickStats,
    ticks: VecDeque<Instant>,
    generation: u64,
    tasks: Vec<(ChunkPos, RegionTask<D>)>,
    /// Sequence numbers of deferred mutations that need this region idle.
    /// Non-empty means the region is reserved and starts no new ticks.
    mutations: BTreeSet<u64>,
}
impl<D> Region<D> {
    fn new(sections: HashSet<SectionPos>, data: D) -> Self {
        Self {
            state: RegionState::Ready,
            merge_targets: HashSet::new(),
            merge_from: HashSet::new(),
            sections,
            data: Some(data),
            next_tick: Instant::now(),
            stats: TickStats::default(),
            ticks: VecDeque::new(),
            generation: 0,
            tasks: Vec::new(),
            mutations: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TickStats {
    pub last_mspt: f64,
    pub tps_5s: f64,
    pub tps_1m: f64,
    /// Ticks dropped because the region fell further behind its schedule
    /// than [`SchedulerConfig::max_catchup`] allows.
    pub skipped_ticks: u64,
}

/// Scheduler tuning; `Default` is 20 TPS with one tick of catch-up on all cores.
#[derive(Debug, Clone, Copy)]
pub struct SchedulerConfig {
    pub threads: usize,
    /// Target interval between ticks of one region (and of the global region).
    pub period: Duration,
    /// How many periods a region's schedule may lag behind real time. When a
    /// region is overloaded its deadline is advanced in whole periods so that
    /// afterwards it runs at most `max_catchup` extra back-to-back ticks instead
    /// of bursting through the whole backlog; dropped periods are counted in
    /// [`TickStats::skipped_ticks`]. `u32::MAX` disables the cap.
    pub max_catchup: u32,
}
impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            threads: thread::available_parallelism().map_or(1, |n| n.get()),
            period: Duration::from_millis(50),
            max_catchup: 1,
        }
    }
}
/// Advances a deadline by one period after dispatch at `now`, dropping whole
/// periods that lag more than `max_catchup` periods. Returns the drop count.
fn advance(deadline: &mut Instant, now: Instant, config: &SchedulerConfig) -> u64 {
    *deadline += config.period;
    let allowed = config.period.saturating_mul(config.max_catchup);
    let Some(floor) = (now + config.period).checked_sub(allowed) else {
        return 0;
    };
    if *deadline >= floor {
        return 0;
    }
    let period = config.period.as_nanos().max(1);
    let skipped = (floor - *deadline).as_nanos().div_ceil(period);
    *deadline += Duration::from_nanos((skipped * period) as u64);
    skipped as u64
}

/// Deadline of a merged region: the section-weighted mean of both deadlines.
/// This conserves total schedule lag across sections, so frequently merging
/// regions gain no priority (taking the earlier deadline let regions with many
/// neighbours starve isolated ones under overload) and lose none.
fn weighted_deadline(a: (Instant, usize), b: (Instant, usize)) -> Instant {
    let (early, late) = if a.0 <= b.0 { (a, b) } else { (b, a) };
    let total = (early.1 + late.1).max(1) as u128;
    let offset = (late.0 - early.0).as_nanos() * late.1 as u128 / total;
    early.0 + Duration::from_nanos(offset as u64)
}

/// Single-threaded topology; transfer it to a `Scheduler` to start ticking.
pub struct Regionizer<D: RegionData> {
    shift: u32,
    radius: i32,
    chunks: HashMap<ChunkPos, usize>,
    sections: HashMap<SectionPos, (usize, RegionId)>,
    regions: HashMap<RegionId, Region<D>>,
    retired: Vec<D>,
    next_id: RegionId,
}
impl<D: RegionData> Default for Regionizer<D> {
    fn default() -> Self {
        Self::new(4, 2)
    }
}
impl<D: RegionData> Regionizer<D> {
    pub fn new(shift: u32, radius: i32) -> Self {
        assert!(shift < 31 && radius >= 1);
        Self {
            shift,
            radius,
            chunks: HashMap::new(),
            sections: HashMap::new(),
            regions: HashMap::new(),
            retired: Vec::new(),
            next_id: 0,
        }
    }
    pub fn section_of(&self, chunk: ChunkPos) -> SectionPos {
        SectionPos(chunk.0 >> self.shift, chunk.1 >> self.shift)
    }
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
    pub fn owner(&self, chunk: ChunkPos) -> Option<RegionId> {
        self.sections.get(&self.section_of(chunk)).map(|s| s.1)
    }
    pub fn data_mut(&mut self, id: RegionId) -> Option<&mut D> {
        self.regions.get_mut(&id)?.data.as_mut()
    }
    fn neighborhood(&self, section: SectionPos) -> Vec<SectionPos> {
        let mut result = Vec::new();
        for dx in -self.radius..=self.radius {
            for dz in -self.radius..=self.radius {
                if let (Some(x), Some(z)) = (section.0.checked_add(dx), section.1.checked_add(dz)) {
                    result.push(SectionPos(x, z));
                }
            }
        }
        result
    }
    fn affected(&self, chunk: ChunkPos) -> HashSet<RegionId> {
        self.neighborhood(self.section_of(chunk))
            .into_iter()
            .filter_map(|s| self.sections.get(&s).map(|entry| entry.1))
            .collect()
    }
    /// Verifies the structural invariants: holder counts match the section
    /// table, every held section is owned by exactly one non-empty region,
    /// sections within the merge radius share a region, and every region is a
    /// single connected component (so it is not merged more than necessary).
    pub fn check_invariants(&self) -> Result<(), String> {
        let mut counts: HashMap<SectionPos, usize> = HashMap::new();
        for (&chunk, &n) in &self.chunks {
            if n == 0 {
                return Err(format!("chunk {chunk:?} has a zero holder count"));
            }
            *counts.entry(self.section_of(chunk)).or_default() += n;
        }
        if counts.len() != self.sections.len() {
            return Err("section table does not match held chunks".into());
        }
        for (section, &(n, id)) in &self.sections {
            if counts.get(section) != Some(&n) {
                return Err(format!("section {section:?} holder count {n} is wrong"));
            }
            if !self
                .regions
                .get(&id)
                .is_some_and(|r| r.sections.contains(section))
            {
                return Err(format!("section {section:?} not in its region {id}"));
            }
            for neighbor in self.neighborhood(*section) {
                if let Some(&(_, other)) = self.sections.get(&neighbor)
                    && other != id
                    && !self.regions[&id].merge_targets.contains(&other)
                    && !self.regions[&other].merge_targets.contains(&id)
                {
                    return Err(format!(
                        "{section:?} (region {id}) and {neighbor:?} (region {other}) are \
                         within merge radius but in different regions"
                    ));
                }
            }
        }
        for (&id, region) in &self.regions {
            let Some(&start) = region.sections.iter().next() else {
                return Err(format!("region {id} is empty"));
            };
            for section in &region.sections {
                if self.sections.get(section).map(|e| e.1) != Some(id) {
                    return Err(format!("region {id} claims foreign section {section:?}"));
                }
            }
            let mut seen = HashSet::from([start]);
            let mut frontier = vec![start];
            while let Some(s) = frontier.pop() {
                for n in self.neighborhood(s) {
                    if region.sections.contains(&n) && seen.insert(n) {
                        frontier.push(n);
                    }
                }
            }
            if seen.len() != region.sections.len() {
                return Err(format!("region {id} is not connected"));
            }
        }
        Ok(())
    }
    /// Whether region `id` is still connected after `removed` left it, proven by
    /// a search bounded to within twice the merge radius of `removed`: the
    /// region stays connected iff the removed section's former neighbours are
    /// still mutually connected, and a local path between them suffices. `false`
    /// means "not proven" and the caller falls back to a full component search.
    fn still_connected(&self, id: RegionId, removed: SectionPos) -> bool {
        let sections = &self.regions[&id].sections;
        let near: Vec<_> = self
            .neighborhood(removed)
            .into_iter()
            .filter(|n| sections.contains(n))
            .collect();
        match near.len() {
            0 => return false,
            1 => return true,
            _ => {}
        }
        let bound = i64::from(self.radius) * 2;
        let local = |s: &SectionPos| {
            (i64::from(s.0) - i64::from(removed.0)).abs() <= bound
                && (i64::from(s.1) - i64::from(removed.1)).abs() <= bound
        };
        let mut unreached: HashSet<_> = near[1..].iter().copied().collect();
        let mut seen = HashSet::from([near[0]]);
        let mut frontier = vec![near[0]];
        while let Some(s) = frontier.pop() {
            for n in self.neighborhood(s) {
                if local(&n) && sections.contains(&n) && seen.insert(n) {
                    unreached.remove(&n);
                    if unreached.is_empty() {
                        return true;
                    }
                    frontier.push(n);
                }
            }
        }
        false
    }
    pub fn add_chunk(&mut self, chunk: ChunkPos) {
        *self.chunks.entry(chunk).or_default() += 1;
        let section = self.section_of(chunk);
        if let Some(entry) = self.sections.get_mut(&section) {
            entry.0 += 1;
            return;
        }
        let mut neighbors: Vec<_> = self.affected(chunk).into_iter().collect();
        neighbors.sort_unstable();
        let id = if let Some(&id) = neighbors.first() {
            id
        } else {
            let id = self.next_id;
            self.next_id += 1;
            self.regions
                .insert(id, Region::new(HashSet::new(), D::default()));
            id
        };
        for other in neighbors.into_iter().skip(1) {
            let old = self.regions.remove(&other).unwrap();
            debug_assert!(old.mutations.is_empty(), "merged a reserved region");
            for s in &old.sections {
                self.sections.get_mut(s).unwrap().1 = id;
            }
            let region = self.regions.get_mut(&id).unwrap();
            region.next_tick = weighted_deadline(
                (region.next_tick, region.sections.len()),
                (old.next_tick, old.sections.len()),
            );
            region.sections.extend(old.sections);
            region.data.as_mut().unwrap().merge(old.data.unwrap());
        }
        self.sections.insert(section, (1, id));
        self.regions.get_mut(&id).unwrap().sections.insert(section);
    }
    pub fn remove_chunk(&mut self, chunk: ChunkPos) -> bool {
        let Some(count) = self.chunks.get_mut(&chunk) else {
            return false;
        };
        *count -= 1;
        if *count == 0 {
            self.chunks.remove(&chunk);
        }
        let section = self.section_of(chunk);
        let entry = self.sections.get_mut(&section).unwrap();
        entry.0 -= 1;
        if entry.0 != 0 {
            return true;
        }
        let id = entry.1;
        self.sections.remove(&section);
        self.regions.get_mut(&id).unwrap().sections.remove(&section);
        if self.regions[&id].sections.is_empty() {
            self.retired
                .push(self.regions.remove(&id).unwrap().data.unwrap());
            return true;
        }
        if self.still_connected(id, section) {
            return true;
        }
        let mut remaining = self.regions[&id].sections.clone();
        let mut components = Vec::new();
        while let Some(&start) = remaining.iter().next() {
            remaining.remove(&start);
            let mut component = HashSet::from([start]);
            let mut frontier = vec![start];
            while let Some(s) = frontier.pop() {
                let neighbors: Vec<_> = self
                    .neighborhood(s)
                    .into_iter()
                    .filter(|n| remaining.contains(n))
                    .collect();
                for n in neighbors {
                    remaining.remove(&n);
                    component.insert(n);
                    frontier.push(n);
                }
            }
            components.push(component);
        }
        let first = components.pop().unwrap();
        // Split pieces inherit the parent's deadline; starting them at `now`
        // would queue them behind every overdue region under overload.
        let next_tick = self.regions[&id].next_tick;
        self.regions.get_mut(&id).unwrap().sections = first;
        for component in components {
            let data = self
                .regions
                .get_mut(&id)
                .unwrap()
                .data
                .as_mut()
                .unwrap()
                .split(&component);
            let new_id = self.next_id;
            self.next_id += 1;
            for s in &component {
                self.sections.get_mut(s).unwrap().1 = new_id;
            }
            let mut region = Region::new(component, data);
            region.next_tick = next_tick;
            self.regions.insert(new_id, region);
        }
        true
    }
}

pub type RegionTask<D> = Box<dyn FnOnce(&mut D) + Send + 'static>;
pub type GlobalTask<G> = Box<dyn FnOnce(&mut G) + Send + 'static>;

#[derive(Clone, Copy)]
enum Mutation {
    Add(ChunkPos),
    Remove(ChunkPos),
}
impl Mutation {
    fn chunk(self) -> ChunkPos {
        match self {
            Self::Add(c) | Self::Remove(c) => c,
        }
    }
}
/// A deferred topology mutation, queued on every region in `regions`.
struct Pending {
    mutation: Mutation,
    section: SectionPos,
    regions: HashSet<RegionId>,
    submitted: Instant,
}
type Deadline = Reverse<(Instant, u64, Option<RegionId>)>;
struct State<D: RegionData, G> {
    topology: Regionizer<D>,
    global: Option<G>,
    global_next: Instant,
    global_generation: u64,
    global_tasks: Vec<GlobalTask<G>>,
    parked: HashMap<SectionPos, Vec<(ChunkPos, RegionTask<D>)>>,
    pending: HashMap<u64, Pending>,
    /// Spatial index of pending mutations, for nearby-conflict lookups only.
    pending_at: HashMap<SectionPos, BTreeSet<u64>>,
    next_mutation: u64,
    deadlines: BinaryHeap<Deadline>,
    stopping: bool,
}
struct Shared<D: RegionData, G> {
    state: Mutex<State<D, G>>,
    wake: Condvar,
}
enum Work<D, G> {
    Region(RegionId, D, Vec<(ChunkPos, RegionTask<D>)>),
    Global(G, Vec<GlobalTask<G>>),
}

/// Returned ownership after all worker threads have stopped.
pub struct Shutdown<D, G> {
    pub regions: Vec<D>,
    pub global: G,
    pub pending_tasks: Vec<(ChunkPos, RegionTask<D>)>,
    pub pending_global_tasks: Vec<GlobalTask<G>>,
}

/// Fixed worker pool. Each region and the global region have independent deadlines.
pub struct Scheduler<D: RegionData, G: Send + 'static> {
    shared: Arc<Shared<D, G>>,
    workers: Vec<JoinHandle<()>>,
}
impl<D: RegionData, G: Send + 'static> Scheduler<D, G> {
    pub fn new(
        topology: Regionizer<D>,
        global: G,
        threads: usize,
        tick: impl Fn(&mut D) + Send + Sync + 'static,
        global_tick: impl Fn(&mut G) + Send + Sync + 'static,
    ) -> Self {
        let config = SchedulerConfig {
            threads,
            ..SchedulerConfig::default()
        };
        Self::with_config(topology, global, config, tick, global_tick)
    }
    pub fn with_config(
        topology: Regionizer<D>,
        global: G,
        config: SchedulerConfig,
        tick: impl Fn(&mut D) + Send + Sync + 'static,
        global_tick: impl Fn(&mut G) + Send + Sync + 'static,
    ) -> Self {
        assert!(config.threads > 0 && !config.period.is_zero());
        let now = Instant::now();
        let mut deadlines = BinaryHeap::new();
        for (&id, r) in &topology.regions {
            deadlines.push(Reverse((r.next_tick, r.generation, Some(id))));
        }
        deadlines.push(Reverse((now, 0, None)));
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                topology,
                global: Some(global),
                global_next: now,
                global_generation: 0,
                global_tasks: Vec::new(),
                parked: HashMap::new(),
                pending: HashMap::new(),
                pending_at: HashMap::new(),
                next_mutation: 0,
                deadlines,
                stopping: false,
            }),
            wake: Condvar::new(),
        });
        let tick = Arc::new(tick);
        let global_tick = Arc::new(global_tick);
        let workers = (0..config.threads)
            .map(|_| {
                let shared = shared.clone();
                let tick = tick.clone();
                let global_tick = global_tick.clone();
                thread::spawn(move || worker(shared, config, tick, global_tick))
            })
            .collect();
        Self { shared, workers }
    }
    pub fn with_default_threads(
        topology: Regionizer<D>,
        global: G,
        tick: impl Fn(&mut D) + Send + Sync + 'static,
        global_tick: impl Fn(&mut G) + Send + Sync + 'static,
    ) -> Self {
        Self::with_config(
            topology,
            global,
            SchedulerConfig::default(),
            tick,
            global_tick,
        )
    }
    fn mutate(&self, mutation: Mutation) {
        let mut s = self.shared.state.lock();
        assert!(!s.stopping, "scheduler is stopping");
        submit(&mut s, mutation);
        self.shared.wake.notify_all();
    }
    /// Adds a holder for `chunk`. Applied immediately unless it involves a
    /// busy region, in which case it is applied at that region's next safe point.
    pub fn add_chunk(&self, chunk: ChunkPos) {
        self.mutate(Mutation::Add(chunk));
    }
    /// Removes a holder for `chunk`; deferred like [`Scheduler::add_chunk`].
    pub fn remove_chunk(&self, chunk: ChunkPos) {
        self.mutate(Mutation::Remove(chunk));
    }
    pub fn schedule_at_chunk(&self, chunk: ChunkPos, task: impl FnOnce(&mut D) + Send + 'static) {
        let mut s = self.shared.state.lock();
        assert!(!s.stopping, "scheduler is stopping");
        route(&mut s, chunk, Box::new(task));
        self.shared.wake.notify_all();
    }
    pub fn schedule_global(&self, task: impl FnOnce(&mut G) + Send + 'static) {
        let mut s = self.shared.state.lock();
        assert!(!s.stopping, "scheduler is stopping");
        s.global_tasks.push(Box::new(task));
        self.shared.wake.notify_all();
    }
    pub fn owner(&self, chunk: ChunkPos) -> Option<RegionId> {
        self.shared.state.lock().topology.owner(chunk)
    }
    pub fn region_count(&self) -> usize {
        self.shared.state.lock().topology.region_count()
    }
    pub fn region_state(&self, id: RegionId) -> Option<RegionState> {
        self.shared
            .state
            .lock()
            .topology
            .regions
            .get(&id)
            .map(|r| r.state)
    }
    pub fn stats(&self, id: RegionId) -> Option<TickStats> {
        self.shared
            .state
            .lock()
            .topology
            .regions
            .get(&id)
            .map(|r| r.stats)
    }
    /// Number of deferred topology mutations not yet applied.
    pub fn pending_mutations(&self) -> usize {
        self.shared.state.lock().pending.len()
    }
    /// Age of the oldest deferred topology mutation, if any.
    pub fn oldest_pending_mutation(&self) -> Option<Duration> {
        let s = self.shared.state.lock();
        s.pending.values().map(|p| p.submitted.elapsed()).max()
    }
    /// Checks topology invariants (see [`Regionizer::check_invariants`]) and
    /// the consistency of the per-region mutation queues.
    pub fn check_invariants(&self) -> Result<(), String> {
        let s = self.shared.state.lock();
        s.topology.check_invariants()?;
        for (&id, region) in &s.topology.regions {
            for m in &region.mutations {
                if !s.pending.get(m).is_some_and(|p| p.regions.contains(&id)) {
                    return Err(format!("region {id} queues unknown mutation {m}"));
                }
            }
        }
        for (m, p) in &s.pending {
            for r in &p.regions {
                if !s
                    .topology
                    .regions
                    .get(r)
                    .is_some_and(|r| r.mutations.contains(m))
                {
                    return Err(format!("mutation {m} missing from region {r} queue"));
                }
            }
            if !s.pending_at.get(&p.section).is_some_and(|i| i.contains(m)) {
                return Err(format!("mutation {m} missing from section index"));
            }
        }
        if s.pending_at.values().map(BTreeSet::len).sum::<usize>() != s.pending.len() {
            return Err("section index has stale entries".into());
        }
        Ok(())
    }
    pub fn shutdown(mut self) -> Shutdown<D, G> {
        {
            let mut s = self.shared.state.lock();
            s.stopping = true;
            self.shared.wake.notify_all();
        }
        for worker in self.workers.drain(..) {
            worker.join().expect("region worker panicked");
        }
        let mut s = self.shared.state.lock();
        // Every region is idle now, so all deferred mutations apply in order.
        let all = s.pending.keys().copied().collect();
        pump(&mut s, all);
        debug_assert!(s.pending.is_empty());
        let mut regions = std::mem::take(&mut s.topology.retired);
        let mut tasks: Vec<_> = std::mem::take(&mut s.parked)
            .into_values()
            .flatten()
            .collect();
        for (_, mut region) in s.topology.regions.drain() {
            regions.push(region.data.take().unwrap());
            tasks.append(&mut region.tasks);
        }
        Shutdown {
            regions,
            global: s.global.take().unwrap(),
            pending_tasks: tasks,
            pending_global_tasks: std::mem::take(&mut s.global_tasks),
        }
    }
}
fn route<D: RegionData, G>(s: &mut State<D, G>, chunk: ChunkPos, task: RegionTask<D>) {
    if let Some(id) = s.topology.owner(chunk) {
        s.topology
            .regions
            .get_mut(&id)
            .unwrap()
            .tasks
            .push((chunk, task));
    } else {
        s.parked
            .entry(s.topology.section_of(chunk))
            .or_default()
            .push((chunk, task));
    }
}
/// Pushes a fresh deadline for `id` if it may tick; older entries become stale.
fn enqueue<D: RegionData, G>(s: &mut State<D, G>, id: RegionId) {
    if let Some(r) = s.topology.regions.get_mut(&id) {
        r.generation += 1;
        if r.data.is_some() && r.mutations.is_empty() && r.state == RegionState::Ready {
            s.deadlines
                .push(Reverse((r.next_tick, r.generation, Some(id))));
        }
    }
}
fn is_free<D: RegionData, G>(s: &State<D, G>, id: RegionId) -> bool {
    s.topology
        .regions
        .get(&id)
        .is_some_and(|r| r.data.is_some() && r.mutations.is_empty())
}
/// Pending mutations whose section lies within the merge radius of `section`.
fn nearby<D: RegionData, G>(s: &State<D, G>, section: SectionPos) -> Vec<u64> {
    s.topology
        .neighborhood(section)
        .iter()
        .filter_map(|n| s.pending_at.get(n))
        .flatten()
        .copied()
        .collect()
}
fn submit<D: RegionData, G>(s: &mut State<D, G>, mutation: Mutation) {
    let chunk = mutation.chunk();
    let section = s.topology.section_of(chunk);
    let affected = s.topology.affected(chunk);
    // Adding a holder to an already-owned section never expands its region.
    if matches!(mutation, Mutation::Add(_))
        && s.topology.sections.contains_key(&section)
        && nearby(s, section).is_empty()
    {
        *s.topology.chunks.entry(chunk).or_default() += 1;
        s.topology.sections.get_mut(&section).unwrap().0 += 1;
        return;
    }
    // ThreadedRegionizer.addChunk / ThreadedRegion.mergeIntoLater: attach new
    // sections to a non-ticking region and defer merges into ticking regions.
    if matches!(mutation, Mutation::Add(_))
        && !s.topology.sections.contains_key(&section)
        && nearby(s, section).is_empty()
        && !affected.is_empty()
        && affected
            .iter()
            .any(|id| s.topology.regions[id].state != RegionState::Ready)
        && affected
            .iter()
            .all(|id| s.topology.regions[id].mutations.is_empty())
    {
        let mut idle: Vec<_> = affected
            .iter()
            .copied()
            .filter(|id| s.topology.regions[id].state != RegionState::Ticking)
            .collect();
        idle.sort_unstable();
        let id = if let Some(id) = idle.first().copied() {
            id
        } else {
            let id = s.topology.next_id;
            s.topology.next_id += 1;
            s.topology
                .regions
                .insert(id, Region::new(HashSet::new(), D::default()));
            id
        };
        for source in idle.into_iter().filter(|source| *source != id) {
            let mut old = s.topology.regions.remove(&source).unwrap();
            for section in &old.sections {
                s.topology.sections.get_mut(section).unwrap().1 = id;
            }
            for target in &old.merge_targets {
                let ticking = s.topology.regions.get_mut(target).unwrap();
                ticking.merge_from.remove(&source);
                ticking.merge_from.insert(id);
            }
            let destination = s.topology.regions.get_mut(&id).unwrap();
            destination.sections.extend(old.sections);
            destination
                .data
                .as_mut()
                .unwrap()
                .merge(old.data.take().unwrap());
            destination.tasks.append(&mut old.tasks);
            destination.merge_targets.extend(old.merge_targets);
        }
        let ticking: Vec<_> = affected
            .iter()
            .copied()
            .filter(|other| {
                *other != id
                    && s.topology
                        .regions
                        .get(other)
                        .is_some_and(|r| r.state == RegionState::Ticking)
            })
            .collect();
        for target in ticking {
            s.topology
                .regions
                .get_mut(&target)
                .unwrap()
                .merge_from
                .insert(id);
            s.topology
                .regions
                .get_mut(&id)
                .unwrap()
                .merge_targets
                .insert(target);
        }
        let region = s.topology.regions.get_mut(&id).unwrap();
        if !region.merge_targets.is_empty() {
            region.state = RegionState::Transient;
            region.generation += 1;
        }
        region.sections.insert(section);
        s.topology.sections.insert(section, (1, id));
        *s.topology.chunks.entry(chunk).or_default() += 1;
        if let Some(tasks) = s.parked.remove(&section) {
            s.topology.regions.get_mut(&id).unwrap().tasks.extend(tasks);
        }
        return;
    }
    if nearby(s, section).is_empty() && affected.iter().all(|&id| is_free(s, id)) {
        apply_topology(s, mutation, &affected);
        return;
    }
    let id = s.next_mutation;
    s.next_mutation += 1;
    s.pending.insert(
        id,
        Pending {
            mutation,
            section,
            regions: HashSet::new(),
            submitted: Instant::now(),
        },
    );
    s.pending_at.entry(section).or_default().insert(id);
    pump(s, BTreeSet::from([id]));
}
/// Checks the given pending mutations (lowest sequence number first), applying
/// each that is ready; applications add newly-unblocked candidates to `work`.
fn pump<D: RegionData, G>(s: &mut State<D, G>, mut work: BTreeSet<u64>) {
    while let Some(id) = work.pop_first() {
        if !s.pending.contains_key(&id) {
            continue;
        }
        refresh(s, id, &mut work);
        if ready(s, id) {
            apply_pending(s, id, &mut work);
        }
    }
}
/// Re-derives the involved regions of a pending mutation from the current
/// topology, reserving new ones and releasing ones no longer involved.
fn refresh<D: RegionData, G>(s: &mut State<D, G>, id: u64, work: &mut BTreeSet<u64>) {
    let chunk = s.pending[&id].mutation.chunk();
    let wanted = s.topology.affected(chunk);
    let old = std::mem::take(&mut s.pending.get_mut(&id).unwrap().regions);
    for &r in old.difference(&wanted) {
        release(s, r, id, work);
    }
    for &r in wanted.difference(&old) {
        s.topology.regions.get_mut(&r).unwrap().mutations.insert(id);
    }
    s.pending.get_mut(&id).unwrap().regions = wanted;
}
fn release<D: RegionData, G>(s: &mut State<D, G>, r: RegionId, id: u64, work: &mut BTreeSet<u64>) {
    let Some(region) = s.topology.regions.get_mut(&r) else {
        return;
    };
    region.mutations.remove(&id);
    match region.mutations.first() {
        None => enqueue(s, r),
        Some(&head) if region.data.is_some() => {
            work.insert(head);
        }
        Some(_) => {}
    }
}
fn ready<D: RegionData, G>(s: &State<D, G>, id: u64) -> bool {
    let p = &s.pending[&id];
    p.regions.iter().all(|r| {
        s.topology
            .regions
            .get(r)
            .is_some_and(|r| r.data.is_some() && r.mutations.first() == Some(&id))
    }) && nearby(s, p.section).into_iter().all(|other| other >= id)
}
fn apply_pending<D: RegionData, G>(s: &mut State<D, G>, id: u64, work: &mut BTreeSet<u64>) {
    let p = s.pending.remove(&id).unwrap();
    if let Some(index) = s.pending_at.get_mut(&p.section) {
        index.remove(&id);
        if index.is_empty() {
            s.pending_at.remove(&p.section);
        }
    }
    // The involved regions may be merged, split or retired: detach every
    // mutation queued on them and re-derive its regions afterwards.
    let mut requeue = BTreeSet::new();
    for r in &p.regions {
        let region = s.topology.regions.get_mut(r).unwrap();
        requeue.extend(std::mem::take(&mut region.mutations));
    }
    requeue.remove(&id);
    for q in &requeue {
        let pending = s.pending.get_mut(q).unwrap();
        pending.regions.retain(|r| !p.regions.contains(r));
    }
    apply_topology(s, p.mutation, &p.regions);
    for &r in &p.regions {
        enqueue(s, r);
    }
    work.extend(requeue);
    work.extend(nearby(s, p.section));
}
/// Applies a mutation whose involved regions (`affected`) are all idle.
fn apply_topology<D: RegionData, G>(
    s: &mut State<D, G>,
    mutation: Mutation,
    affected: &HashSet<RegionId>,
) {
    let chunk = mutation.chunk();
    let section = s.topology.section_of(chunk);
    let mut tasks = Vec::new();
    for id in affected {
        tasks.append(&mut s.topology.regions.get_mut(id).unwrap().tasks);
    }
    let first_new = s.topology.next_id;
    match mutation {
        Mutation::Add(c) => s.topology.add_chunk(c),
        Mutation::Remove(c) => {
            s.topology.remove_chunk(c);
        }
    }
    if let Some(mut parked) = s.parked.remove(&section) {
        tasks.append(&mut parked);
    }
    for (chunk, task) in tasks {
        route(s, chunk, task);
    }
    for id in first_new..s.topology.next_id {
        enqueue(s, id);
    }
}
fn worker<D: RegionData, G: Send + 'static>(
    shared: Arc<Shared<D, G>>,
    config: SchedulerConfig,
    tick: Arc<impl Fn(&mut D) + Send + Sync>,
    global_tick: Arc<impl Fn(&mut G) + Send + Sync>,
) {
    loop {
        let work = {
            let mut s = shared.state.lock();
            loop {
                if s.stopping {
                    return;
                }
                let Some(&Reverse((deadline, generation, id))) = s.deadlines.peek() else {
                    shared.wake.wait(&mut s);
                    continue;
                };
                let valid = match id {
                    Some(id) => s.topology.regions.get(&id).is_some_and(|r| {
                        r.generation == generation
                            && r.data.is_some()
                            && r.mutations.is_empty()
                            && r.state == RegionState::Ready
                    }),
                    None => s.global.is_some() && generation == s.global_generation,
                };
                if !valid {
                    s.deadlines.pop();
                    continue;
                }
                let now = Instant::now();
                if deadline > now {
                    shared.wake.wait_for(&mut s, deadline - now);
                    continue;
                }
                s.deadlines.pop();
                break if let Some(id) = id {
                    let r = s.topology.regions.get_mut(&id).unwrap();
                    r.state = RegionState::Ticking;
                    r.stats.skipped_ticks += advance(&mut r.next_tick, now, &config);
                    Work::Region(id, r.data.take().unwrap(), std::mem::take(&mut r.tasks))
                } else {
                    advance(&mut s.global_next, now, &config);
                    Work::Global(
                        s.global.take().unwrap(),
                        std::mem::take(&mut s.global_tasks),
                    )
                };
            }
        };
        let started = Instant::now();
        match work {
            Work::Region(id, mut data, tasks) => {
                for (_, task) in tasks {
                    task(&mut data);
                }
                tick(&mut data);
                let now = Instant::now();
                let mut s = shared.state.lock();
                let r = s.topology.regions.get_mut(&id).unwrap();
                r.data = Some(data);
                r.state = RegionState::Ready;
                r.stats.last_mspt = started.elapsed().as_secs_f64() * 1000.0;
                r.ticks.push_back(now);
                while r
                    .ticks
                    .front()
                    .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60))
                {
                    r.ticks.pop_front();
                }
                r.stats.tps_5s = r
                    .ticks
                    .iter()
                    .filter(|t| now.duration_since(**t) <= Duration::from_secs(5))
                    .count() as f64
                    / 5.0;
                r.stats.tps_1m = r.ticks.len() as f64 / 60.0;
                // ThreadedRegion.markNotTicking merges transient sources at the safe point.
                let incoming = std::mem::take(&mut r.merge_from);
                for source in incoming {
                    let mut old = s.topology.regions.remove(&source).unwrap();
                    old.merge_targets.remove(&id);
                    for target in &old.merge_targets {
                        let ticking = s.topology.regions.get_mut(target).unwrap();
                        ticking.merge_from.remove(&source);
                        ticking.merge_from.insert(id);
                    }
                    if !old.merge_targets.is_empty() {
                        let destination = s.topology.regions.get_mut(&id).unwrap();
                        destination.state = RegionState::Transient;
                        destination.merge_targets.extend(old.merge_targets.drain());
                    }
                    // Pending removals may have reserved this transient owner.
                    // Transfer their reservation before refreshing their topology.
                    for mutation in &old.mutations {
                        let pending = s.pending.get_mut(mutation).unwrap();
                        pending.regions.remove(&source);
                        pending.regions.insert(id);
                    }
                    s.topology
                        .regions
                        .get_mut(&id)
                        .unwrap()
                        .mutations
                        .append(&mut old.mutations);
                    for section in &old.sections {
                        s.topology.sections.get_mut(section).unwrap().1 = id;
                    }
                    let destination = s.topology.regions.get_mut(&id).unwrap();
                    destination.sections.extend(old.sections);
                    destination
                        .data
                        .as_mut()
                        .unwrap()
                        .merge(old.data.take().unwrap());
                    destination.tasks.append(&mut old.tasks);
                }
                let r = s.topology.regions.get_mut(&id).unwrap();
                // Post-tick safe point: apply mutations waiting on this region.
                if let Some(&head) = r.mutations.first() {
                    pump(&mut s, BTreeSet::from([head]));
                }
                enqueue(&mut s, id);
                shared.wake.notify_all();
            }
            Work::Global(mut data, tasks) => {
                for task in tasks {
                    task(&mut data);
                }
                global_tick(&mut data);
                let mut s = shared.state.lock();
                s.global = Some(data);
                s.global_generation += 1;
                let entry = Reverse((s.global_next, s.global_generation, None));
                s.deadlines.push(entry);
                shared.wake.notify_all();
            }
        }
    }
}
impl<D: RegionData, G: Send + 'static> Drop for Scheduler<D, G> {
    fn drop(&mut self) {
        {
            let mut s = self.shared.state.lock();
            s.stopping = true;
            self.shared.wake.notify_all();
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Unit;
    impl RegionData for Unit {
        fn merge(&mut self, _: Self) {}
        fn split(&mut self, _: &HashSet<SectionPos>) -> Self {
            Self
        }
    }

    // Regression: split pieces used to restart at `now`, starving them behind
    // overdue regions when the scheduler is overloaded; merges used to take the
    // earlier deadline, favouring regions with many neighbours.
    #[test]
    fn split_and_merge_preserve_overdue_deadlines() {
        let mut r = Regionizer::<Unit>::new(0, 1);
        for x in [0, 1, 2] {
            r.add_chunk(ChunkPos(x, 0));
        }
        let id = r.owner(ChunkPos(0, 0)).unwrap();
        let overdue = Instant::now() - Duration::from_secs(10);
        r.regions.get_mut(&id).unwrap().next_tick = overdue;
        r.remove_chunk(ChunkPos(1, 0));
        assert_eq!(r.region_count(), 2);
        for region in r.regions.values() {
            assert_eq!(region.next_tick, overdue);
        }
        let right = r.owner(ChunkPos(2, 0)).unwrap();
        let later = overdue + Duration::from_secs(5);
        r.regions.get_mut(&right).unwrap().next_tick = later;
        // One section on each side (the bridge is inserted after merging): the
        // section-weighted mean is halfway.
        r.add_chunk(ChunkPos(1, 0));
        let merged = r.owner(ChunkPos(0, 0)).unwrap();
        let expected = overdue + Duration::from_secs(5) / 2;
        let got = r.regions[&merged].next_tick;
        assert!(got.max(expected) - got.min(expected) < Duration::from_millis(1));
        r.check_invariants().unwrap();
    }

    // The bounded local search must never report a split region as connected,
    // and splits / non-splits must match a brute-force component count.
    #[test]
    fn local_connectivity_check_matches_full_search() {
        for radius in [1, 2] {
            let mut r = Regionizer::<Unit>::new(0, radius);
            let mut held = Vec::new();
            let mut seed = 0x2545_f491_4f6c_dd1du64;
            for _ in 0..800 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let chunk = ChunkPos((seed % 16) as i32, ((seed >> 16) % 16) as i32);
                if !(seed >> 40).is_multiple_of(3) || held.is_empty() {
                    r.add_chunk(chunk);
                    held.push(chunk);
                } else {
                    let i = (seed >> 44) as usize % held.len();
                    r.remove_chunk(held.swap_remove(i));
                }
                r.check_invariants().unwrap();
            }
        }
        // A ring: removing one section is only provably safe through the long
        // way round, so the fallback must keep it as one region.
        let mut r = Regionizer::<Unit>::new(0, 1);
        for i in 0..10 {
            for (x, z) in [(i, 0), (i, 9), (0, i), (9, i)] {
                r.add_chunk(ChunkPos(x, z));
            }
        }
        r.remove_chunk(ChunkPos(5, 0));
        assert_eq!(r.region_count(), 1);
        r.remove_chunk(ChunkPos(5, 9));
        assert_eq!(r.region_count(), 2);
        r.check_invariants().unwrap();
    }
}
