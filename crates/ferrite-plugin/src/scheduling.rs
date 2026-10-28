/// A region task ready for the server's region scheduler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionTask {
    pub id: i64,
    pub world: String,
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub region_aware: bool,
}
/// An entity task to be routed by the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityTask {
    pub id: i64,
    pub entity: String,
    pub region_aware: bool,
}
#[derive(Default)]
pub(crate) struct Queue {
    tick: u64,
    pending: Vec<(u64, RegionTask)>,
    globals: Vec<(u64, i64)>,
    entities: Vec<(u64, EntityTask)>,
}
impl Queue {
    pub(crate) fn region(
        &mut self,
        id: i64,
        world: String,
        chunk_x: i32,
        chunk_z: i32,
        delay: u64,
        region_aware: bool,
    ) {
        self.pending.push((
            self.tick.saturating_add(delay.max(1)),
            RegionTask {
                id,
                world,
                chunk_x,
                chunk_z,
                region_aware,
            },
        ));
    }
    pub(crate) fn global(&mut self, id: i64, delay: u64) {
        self.globals
            .push((self.tick.saturating_add(delay.max(1)), id));
    }
    pub(crate) fn entity(&mut self, id: i64, entity: String, delay: u64, region_aware: bool) {
        self.entities.push((
            self.tick.saturating_add(delay.max(1)),
            EntityTask {
                id,
                entity,
                region_aware,
            },
        ));
    }
    pub(crate) fn current_tick(&self) -> u64 {
        self.tick
    }
    pub(crate) fn advance(&mut self) -> (Vec<RegionTask>, Vec<i64>, Vec<EntityTask>) {
        self.tick = self.tick.saturating_add(1);
        let mut ready = Vec::new();
        self.pending.retain(|(due, task)| {
            if *due <= self.tick {
                ready.push(task.clone());
                false
            } else {
                true
            }
        });
        let mut globals = Vec::new();
        self.globals.retain(|(due, id)| {
            if *due <= self.tick {
                globals.push(*id);
                false
            } else {
                true
            }
        });
        let mut entities = Vec::new();
        self.entities.retain(|(due, task)| {
            if *due <= self.tick {
                entities.push(task.clone());
                false
            } else {
                true
            }
        });
        (ready, globals, entities)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entity_task_waits_until_due() {
        let mut queue = Queue::default();
        queue.entity(2, "player-id".into(), 1, false);
        let (_, _, entities) = queue.advance();
        assert_eq!(entities[0].entity, "player-id");
        assert!(!entities[0].region_aware);
    }
    #[test]
    fn region_task_retains_coordinates_and_waits_for_due_tick() {
        let mut queue = Queue::default();
        queue.region(17, "world".into(), -4, 8, 2, true);
        assert!(queue.advance().0.is_empty());
        let tasks = queue.advance().0;
        assert_eq!(
            tasks,
            vec![RegionTask {
                id: 17,
                world: "world".into(),
                chunk_x: -4,
                chunk_z: 8,
                region_aware: true
            }]
        );
        assert!(queue.advance().0.is_empty());
    }
}
