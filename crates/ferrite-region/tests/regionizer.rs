use ferrite_region::{ChunkPos, RegionData, Regionizer, SectionPos};
use std::collections::HashSet;

#[derive(Default)]
struct Data(HashSet<SectionPos>);
impl RegionData for Data {
    fn merge(&mut self, other: Self) {
        self.0.extend(other.0);
    }
    fn split(&mut self, sections: &HashSet<SectionPos>) -> Self {
        let moved = self.0.intersection(sections).copied().collect();
        self.0.retain(|s| !sections.contains(s));
        Self(moved)
    }
}
#[test]
fn bridge_merges_and_removal_splits() {
    let mut r = Regionizer::<Data>::new(0, 1);
    r.add_chunk(ChunkPos(0, 0));
    r.add_chunk(ChunkPos(2, 0));
    assert_eq!(r.region_count(), 2);
    r.add_chunk(ChunkPos(1, 0));
    assert_eq!(r.region_count(), 1);
    r.remove_chunk(ChunkPos(1, 0));
    assert_eq!(r.region_count(), 2);
}

#[test]
fn queued_tasks_follow_chunk_ownership_and_shutdown_returns_data() {
    use ferrite_region::Scheduler;
    use std::sync::mpsc;
    use std::time::Duration;
    let scheduler = Scheduler::new(
        Regionizer::<Data>::new(0, 1),
        (),
        2,
        |_: &mut Data| {},
        |_: &mut ()| {},
    );
    let (tx, rx) = mpsc::channel();
    scheduler.schedule_at_chunk(ChunkPos(8, 0), move |_| {
        tx.send(8).unwrap();
    });
    assert!(rx.recv_timeout(Duration::from_millis(80)).is_err());
    scheduler.add_chunk(ChunkPos(8, 0));
    assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 8);
    let result = scheduler.shutdown();
    assert_eq!(result.regions.len(), 1);
    assert!(result.pending_tasks.is_empty());
}

#[test]
fn split_partitions_data_and_sections_use_floor_division() {
    let mut r = Regionizer::<Data>::new(1, 1);
    assert_eq!(r.section_of(ChunkPos(-1, -3)), SectionPos(-1, -2));
    for x in [0, 2, 4] {
        r.add_chunk(ChunkPos(x, 0));
    }
    let id = r.owner(ChunkPos(0, 0)).unwrap();
    r.data_mut(id)
        .unwrap()
        .0
        .extend([SectionPos(0, 0), SectionPos(2, 0)]);
    r.remove_chunk(ChunkPos(2, 0));
    let left = r.owner(ChunkPos(0, 0)).unwrap();
    let right = r.owner(ChunkPos(4, 0)).unwrap();
    assert_ne!(left, right);
    assert_eq!(
        r.data_mut(left).unwrap().0,
        HashSet::from([SectionPos(0, 0)])
    );
    assert_eq!(
        r.data_mut(right).unwrap().0,
        HashSet::from([SectionPos(2, 0)])
    );
}

#[test]
fn mutations_wait_for_inflight_tick_and_tasks_follow_merge() {
    use ferrite_region::Scheduler;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::time::Duration;
    let mut topology = Regionizer::<Data>::new(0, 1);
    topology.add_chunk(ChunkPos(0, 0));
    topology.add_chunk(ChunkPos(2, 0));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release = Arc::new(std::sync::Mutex::new(release_rx));
    let first = Arc::new(AtomicBool::new(true));
    let scheduler = Scheduler::new(
        topology,
        (),
        2,
        move |_| {
            if first.swap(false, Ordering::SeqCst) {
                entered_tx.send(()).unwrap();
                release.lock().unwrap().recv().unwrap();
            }
        },
        |_| {},
    );
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    scheduler.add_chunk(ChunkPos(1, 0));
    assert_eq!(scheduler.region_count(), 2);
    let (tx, rx) = mpsc::channel();
    scheduler.schedule_at_chunk(ChunkPos(2, 0), move |_| {
        tx.send(()).unwrap();
    });
    release_tx.send(()).unwrap();
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(scheduler.region_count(), 1);
    assert_eq!(scheduler.shutdown().regions.len(), 1);
}

#[test]
fn holder_counts_and_global_queue_are_preserved() {
    use ferrite_region::Scheduler;
    use std::sync::mpsc;
    use std::time::Duration;
    let mut r = Regionizer::<Data>::default();
    r.add_chunk(ChunkPos(0, 0));
    r.add_chunk(ChunkPos(0, 0));
    assert!(r.remove_chunk(ChunkPos(0, 0)));
    assert!(r.owner(ChunkPos(0, 0)).is_some());
    let scheduler = Scheduler::new(r, 0usize, 1, |_| {}, |_| {});
    let (tx, rx) = mpsc::channel();
    scheduler.schedule_global(move |value| {
        *value += 1;
        tx.send(()).unwrap();
    });
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(scheduler.shutdown().global, 1);
}

#[test]
#[ignore = "long-running concurrent stress test"]
fn stress_mutations_and_queued_tasks() {
    use ferrite_region::Scheduler;
    use std::sync::mpsc;
    use std::time::Duration;
    let scheduler = Scheduler::new(Regionizer::<Data>::new(0, 1), (), 4, |_| {}, |_| {});
    let mut held = [false; 64];
    let mut seed = 1u32;
    for _ in 0..5000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let i = (seed as usize >> 16) % held.len();
        let chunk = ChunkPos((i * 4) as i32, 0);
        if held[i] {
            scheduler.remove_chunk(chunk);
        } else {
            scheduler.add_chunk(chunk);
        }
        held[i] = !held[i];
    }
    let (tx, rx) = mpsc::channel();
    for (i, is_held) in held.iter().enumerate() {
        let chunk = ChunkPos((i * 4) as i32, 0);
        if !is_held {
            scheduler.add_chunk(chunk);
        }
        let tx = tx.clone();
        scheduler.schedule_at_chunk(chunk, move |_| {
            tx.send(i).unwrap();
        });
    }
    drop(tx);
    let mut received = HashSet::new();
    for _ in 0..64 {
        assert!(received.insert(rx.recv_timeout(Duration::from_secs(10)).unwrap()));
    }
    for i in 0..64 {
        assert!(scheduler.owner(ChunkPos(i * 4, 0)).is_some());
    }
    assert_eq!(scheduler.region_count(), 64);
    assert!(scheduler.shutdown().pending_tasks.is_empty());
}

#[test]
fn tasks_run_on_the_region_that_owns_the_target() {
    use ferrite_region::Scheduler;
    use std::sync::mpsc;
    use std::time::Duration;
    let mut r = Regionizer::<Data>::new(0, 1);
    r.add_chunk(ChunkPos(0, 0));
    r.add_chunk(ChunkPos(10, 0));
    let left = r.owner(ChunkPos(0, 0)).unwrap();
    let right = r.owner(ChunkPos(10, 0)).unwrap();
    r.data_mut(left).unwrap().0.insert(SectionPos(0, 0));
    r.data_mut(right).unwrap().0.insert(SectionPos(10, 0));
    let scheduler = Scheduler::new(r, (), 2, |_| {}, |_| {});
    let (tx, rx) = mpsc::channel();
    scheduler.schedule_at_chunk(ChunkPos(10, 0), move |data| {
        tx.send(data.0.clone()).unwrap();
    });
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        HashSet::from([SectionPos(10, 0)])
    );
    scheduler.schedule_at_chunk(ChunkPos(100, 0), |_| {});
    let result = scheduler.shutdown();
    assert_eq!(result.regions.len(), 2);
    assert_eq!(result.pending_tasks.len(), 1);
}
