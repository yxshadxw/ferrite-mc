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

/// World-owned state. `split` removes and returns data belonging to `sections`.
pub trait RegionData: Default + Send + 'static {
    fn merge(&mut self, other: Self);
    fn split(&mut self, sections: &HashSet<SectionPos>) -> Self;
}

struct Region<D> {
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
            region.sections.extend(old.sections);
            region.data.as_mut().unwrap().merge(old.data.unwrap());
            // Keep the more overdue deadline so a merge never loses tick debt.
            region.next_tick = region.next_tick.min(old.next_tick);
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
        assert!(threads > 0);
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
        let workers = (0..threads)
            .map(|_| {
                let shared = shared.clone();
                let tick = tick.clone();
                let global_tick = global_tick.clone();
                thread::spawn(move || worker(shared, tick, global_tick))
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
        Self::new(
            topology,
            global,
            thread::available_parallelism().map_or(1, |n| n.get()),
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
        if r.data.is_some() && r.mutations.is_empty() {
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
    tick: Arc<impl Fn(&mut D) + Send + Sync>,
    global_tick: Arc<impl Fn(&mut G) + Send + Sync>,
) {
    const PERIOD: Duration = Duration::from_millis(50);
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
                        r.generation == generation && r.data.is_some() && r.mutations.is_empty()
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
                    r.next_tick += PERIOD;
                    Work::Region(id, r.data.take().unwrap(), std::mem::take(&mut r.tasks))
                } else {
                    s.global_next += PERIOD;
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
    // overdue regions when the scheduler is overloaded.
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
        r.add_chunk(ChunkPos(1, 0));
        let merged = r.owner(ChunkPos(0, 0)).unwrap();
        assert_eq!(r.regions[&merged].next_tick, overdue);
        r.check_invariants().unwrap();
    }
}
