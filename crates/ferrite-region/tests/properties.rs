use ferrite_region::{ChunkPos, RegionData, Regionizer, RegionizerConfig, SectionPos};
use proptest::prelude::*;
use std::collections::{HashMap, HashSet};
#[derive(Default)]
struct Data;
impl RegionData for Data {
    fn merge(&mut self, _: Self) {}
    fn split(&mut self, _: &HashSet<SectionPos>) -> Self {
        Self
    }
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn every_held_chunk_has_one_owner(ops in prop::collection::vec((-12i32..12,-12i32..12,any::<bool>()),1..100)) {
        let mut topology=Regionizer::<Data>::with_config(RegionizerConfig {section_shift:1,merge_radius:1,empty_section_radius:1,min_section_recalc_count:2,max_dead_section_percent:0.4});
        let mut held=HashMap::<ChunkPos,usize>::new();
        for (x,z,add) in ops {
            let chunk=ChunkPos(x,z);
            if add { topology.add_chunk(chunk); *held.entry(chunk).or_default()+=1; }
            else if let Some(count)=held.get_mut(&chunk) { *count-=1; if *count==0 { held.remove(&chunk); } prop_assert!(topology.remove_chunk(chunk)); }
            topology.check_invariants().map_err(TestCaseError::fail)?;
            for (&chunk,&count) in &held { prop_assert!(topology.owner(chunk).is_some()); prop_assert_eq!(topology.holder_count(chunk),count); }
        }
        topology.recalculate();
        topology.check_invariants().map_err(TestCaseError::fail)?;
    }
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(12))]
    #[test]
    fn tasks_survive_bridge_merge_and_split(values in prop::collection::vec(0u8..64,1..40)) {
        use ferrite_region::Scheduler;
        use std::{sync::mpsc,time::Duration};
        let mut topology=Regionizer::<Data>::new(0,1);
        topology.add_chunk(ChunkPos(0,0)); topology.add_chunk(ChunkPos(2,0));
        let scheduler=Scheduler::new(topology,(),2,|_|{},|_|{});
        let (tx,rx)=mpsc::channel();
        for (id,value) in values.iter().enumerate() {
            let tx=tx.clone(); let chunk=if value%2==0 {ChunkPos(0,0)} else {ChunkPos(2,0)};
            scheduler.schedule_at_chunk(chunk,move |_| { tx.send(id).unwrap(); });
        }
        scheduler.add_chunk(ChunkPos(1,0)); scheduler.remove_chunk(ChunkPos(1,0));
        let mut received=HashSet::new();
        for _ in 0..values.len() { let id=rx.recv_timeout(Duration::from_secs(3)).map_err(|e|TestCaseError::fail(e.to_string()))?; prop_assert!(received.insert(id)); }
        prop_assert_eq!(received.len(),values.len());
        prop_assert!(scheduler.shutdown().pending_tasks.is_empty());
    }
}
