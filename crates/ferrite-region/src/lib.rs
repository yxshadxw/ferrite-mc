//! Region ownership and independent ticking for chunk sections.
//!
//! Sections use floor division, including for negative chunk coordinates. Two
//! sections whose Chebyshev distance is at most the merge radius share a region.
//! Each held chunk belongs to exactly one section and each section to one region.
//! The scheduler temporarily takes region data out of the topology while ticking;
//! topology mutations are deferred until no tick is in flight, and no new tick
//! starts while mutations are pending. Tasks retain their chunk address until a
//! tick claims them, so topology changes cannot strand them in the wrong region.

use parking_lot::{Condvar, Mutex};
use std::collections::{HashMap, HashSet, VecDeque};
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
    fn adjacent(&self, a: SectionPos, b: SectionPos) -> bool {
        (i64::from(a.0) - i64::from(b.0)).abs() <= i64::from(self.radius)
            && (i64::from(a.1) - i64::from(b.1)).abs() <= i64::from(self.radius)
    }
    pub fn add_chunk(&mut self, chunk: ChunkPos) {
        *self.chunks.entry(chunk).or_default() += 1;
        let section = self.section_of(chunk);
        if let Some(entry) = self.sections.get_mut(&section) {
            entry.0 += 1;
            return;
        }
        let mut neighbors: Vec<_> = self
            .sections
            .iter()
            .filter_map(|(&s, &(_, id))| self.adjacent(s, section).then_some(id))
            .collect();
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
                let neighbors: Vec<_> = remaining
                    .iter()
                    .copied()
                    .filter(|&x| self.adjacent(s, x))
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
                },
            );
        }
        true
    }
}

pub type RegionTask<D> = Box<dyn FnOnce(&mut D) + Send + 'static>;
pub type GlobalTask<G> = Box<dyn FnOnce(&mut G) + Send + 'static>;
enum Mutation {
    Add(ChunkPos),
    Remove(ChunkPos),
}
struct State<D: RegionData, G> {
    topology: Regionizer<D>,
    global: Option<G>,
    global_next: Instant,
    global_tasks: Vec<GlobalTask<G>>,
    tasks: Vec<(ChunkPos, RegionTask<D>)>,
    mutations: VecDeque<Mutation>,
    active: usize,
    stopping: bool,
}
struct Shared<D: RegionData, G> {
    state: Mutex<State<D, G>>,
    wake: Condvar,
}
enum Work<D, G> {
    Region(RegionId, D, Vec<RegionTask<D>>),
    Global(G, Vec<GlobalTask<G>>),
}

/// Returned ownership after all worker threads have stopped. Tasks for chunks
/// which never acquired an owner remain available to the caller.
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
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                topology,
                global: Some(global),
                global_next: Instant::now(),
                global_tasks: Vec::new(),
                tasks: Vec::new(),
                mutations: VecDeque::new(),
                active: 0,
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
        if s.active == 0 {
            apply_mutations(&mut s);
        }
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
        s.tasks.push((chunk, Box::new(task)));
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
        regions.extend(s.topology.regions.drain().map(|(_, r)| r.data.unwrap()));
        Shutdown {
            regions,
            global: s.global.take().unwrap(),
            pending_tasks: std::mem::take(&mut s.tasks),
            pending_global_tasks: std::mem::take(&mut s.global_tasks),
        }
    }
}
fn apply_mutations<D: RegionData, G>(s: &mut State<D, G>) {
    while let Some(m) = s.mutations.pop_front() {
        match m {
            Mutation::Add(c) => s.topology.add_chunk(c),
            Mutation::Remove(c) => {
                s.topology.remove_chunk(c);
            }
        }
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
                if !s.mutations.is_empty() {
                    if s.active == 0 {
                        apply_mutations(&mut s);
                    } else {
                        shared.wake.wait(&mut s);
                        continue;
                    }
                }
                let now = Instant::now();
                let candidate = s
                    .topology
                    .regions
                    .iter()
                    .filter(|(_, r)| r.data.is_some())
                    .min_by_key(|(_, r)| r.next_tick)
                    .map(|(&id, r)| (id, r.next_tick));
                let global_due = s.global.is_some().then_some(s.global_next);
                let next = match (candidate, global_due) {
                    (Some((_id, time)), Some(g)) if g <= time => Some((None, g)),
                    (Some((id, time)), _) => Some((Some(id), time)),
                    (None, Some(g)) => Some((None, g)),
                    _ => None,
                };
                if let Some((id, deadline)) = next {
                    if deadline > now {
                        shared.wake.wait_for(&mut s, deadline - now);
                        continue;
                    }
                    s.active += 1;
                    break if let Some(id) = id {
                        let r = s.topology.regions.get_mut(&id).unwrap();
                        r.next_tick += PERIOD;
                        let data = r.data.take().unwrap();
                        let mut ready = Vec::new();
                        let mut pending = Vec::new();
                        for (chunk, task) in std::mem::take(&mut s.tasks) {
                            if s.topology.owner(chunk) == Some(id) {
                                ready.push(task);
                            } else {
                                pending.push((chunk, task));
                            }
                        }
                        s.tasks = pending;
                        Work::Region(id, data, ready)
                    } else {
                        s.global_next += PERIOD;
                        Work::Global(
                            s.global.take().unwrap(),
                            std::mem::take(&mut s.global_tasks),
                        )
                    };
                }
                shared.wake.wait(&mut s);
            }
        };
        let started = Instant::now();
        match work {
            Work::Region(id, mut data, tasks) => {
                for task in tasks {
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
                s.active -= 1;
                shared.wake.notify_all();
            }
            Work::Global(mut data, tasks) => {
                for task in tasks {
                    task(&mut data);
                }
                global_tick(&mut data);
                let mut s = shared.state.lock();
                s.global = Some(data);
                s.active -= 1;
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
