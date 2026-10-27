//! Region ownership and independent ticking for chunk sections.
//!
//! Sections use floor division, including for negative chunk coordinates. Two
//! sections whose Chebyshev distance is at most the merge radius share a region.
//! Each held chunk belongs to exactly one section and each section to one region.
//! The scheduler temporarily takes region data out of the topology while ticking;
//! mutations wait only for the regions they affect. A pending mutation reserves
//! those regions until their data returns, while unrelated regions keep ticking.
//! The sole metadata mutex is never held while executing ticks or tasks; there
//! are no other scheduler locks, so tick callbacks cannot form a lock cycle.
//! Tasks retain their chunk address and are rerouted on topology changes.

use parking_lot::{Condvar, Mutex};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};
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
    pub fn add_chunk(&mut self, chunk: ChunkPos) {
        *self.chunks.entry(chunk).or_default() += 1;
        let section = self.section_of(chunk);
        if let Some(entry) = self.sections.get_mut(&section) {
            entry.0 += 1;
            return;
        }
        let mut neighbors: Vec<_> = self.affected(chunk).into_iter().collect();
        neighbors.sort_unstable();
        neighbors.dedup();
        let id = if let Some(&id) = neighbors.first() {
            id
        } else {
            let id = self.next_id;
            self.next_id += 1;
            self.regions.insert(
                id,
                Region {
                    sections: HashSet::new(),
                    data: Some(D::default()),
                    next_tick: Instant::now(),
                    stats: TickStats::default(),
                    ticks: VecDeque::new(),
                    generation: 0,
                    tasks: Vec::new(),
                },
            );
            id
        };
        for other in neighbors.into_iter().skip(1) {
            let old = self.regions.remove(&other).unwrap();
            for s in &old.sections {
                self.sections.get_mut(s).unwrap().1 = id;
            }
            let region = self.regions.get_mut(&id).unwrap();
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
            self.regions.insert(
                new_id,
                Region {
                    sections: component,
                    data: Some(data),
                    next_tick: Instant::now(),
                    stats: TickStats::default(),
                    ticks: VecDeque::new(),
                    generation: 0,
                    tasks: Vec::new(),
                },
            );
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
type Deadline = Reverse<(Instant, u64, Option<RegionId>)>;
struct State<D: RegionData, G> {
    topology: Regionizer<D>,
    global: Option<G>,
    global_next: Instant,
    global_generation: u64,
    global_tasks: Vec<GlobalTask<G>>,
    parked: HashMap<SectionPos, Vec<(ChunkPos, RegionTask<D>)>>,
    mutations: VecDeque<Mutation>,
    blocked: HashSet<RegionId>,
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
                mutations: VecDeque::new(),
                blocked: HashSet::new(),
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
        s.mutations.push_back(mutation);
        apply_mutations(&mut s);
        self.shared.wake.notify_all();
    }
    pub fn add_chunk(&self, chunk: ChunkPos) {
        self.mutate(Mutation::Add(chunk));
    }
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
        apply_mutations(&mut s);
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
fn enqueue<D: RegionData, G>(s: &mut State<D, G>, id: RegionId) {
    if let Some(r) = s.topology.regions.get_mut(&id) {
        r.generation += 1;
        if r.data.is_some() && !s.blocked.contains(&id) {
            s.deadlines
                .push(Reverse((r.next_tick, r.generation, Some(id))));
        }
    }
}
// Pending operations reserve their involved regions. Later operations touching
// the same region or overlapping section neighborhoods retain submission order.
fn apply_mutations<D: RegionData, G>(s: &mut State<D, G>) {
    let old_blocked = std::mem::take(&mut s.blocked);
    let mut blocked = HashSet::new();
    let mut reserved_sections: Vec<SectionPos> = Vec::new();
    let mut pending = VecDeque::new();
    while let Some(mutation) = s.mutations.pop_front() {
        let chunk = mutation.chunk();
        let section = s.topology.section_of(chunk);
        let affected = s.topology.affected(chunk);
        let distance = i64::from(s.topology.radius) * 2;
        let overlaps = reserved_sections.iter().any(|p| {
            (i64::from(p.0) - i64::from(section.0)).abs() <= distance
                && (i64::from(p.1) - i64::from(section.1)).abs() <= distance
        });
        if overlaps
            || !affected.is_disjoint(&blocked)
            || affected
                .iter()
                .any(|id| s.topology.regions[id].data.is_none())
        {
            blocked.extend(affected);
            reserved_sections.push(section);
            pending.push_back(mutation);
            continue;
        }
        let mut tasks = Vec::new();
        for id in &affected {
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
    s.mutations = pending;
    s.blocked = blocked;
    for id in old_blocked
        .symmetric_difference(&s.blocked)
        .copied()
        .collect::<Vec<_>>()
    {
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
                        r.generation == generation && r.data.is_some() && !s.blocked.contains(&id)
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
                apply_mutations(&mut s);
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
