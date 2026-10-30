//! Region-local random ticking. Sampling follows ServerLevel.tickChunk; empty
//! sections are skipped as in Lithium's chunk-section random-tick counters.
use super::{anvil::AnvilChunkSource, fluid::RegionFluidWorld, region::Tracking};
use ferrite_fluid::FluidWorld;
use ferrite_noise::random::{RandomSource, XoroshiroRandomSource};
use ferrite_region::ChunkPos;
use ferrite_world::Chunk;
use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Debug)]
struct Event {
    pos: (i32, i32, i32),
    state: u32,
    block_tick: bool,
    fluid_tick: bool,
}
type Handler = fn(&mut RegionFluidWorld<'_>, (i32, i32, i32), u32, &mut XoroshiroRandomSource);
fn no_op(_: &mut RegionFluidWorld<'_>, _: (i32, i32, i32), _: u32, _: &mut XoroshiroRandomSource) {}
fn handlers() -> &'static HashMap<u32, Handler> {
    static TABLE: OnceLock<HashMap<u32, Handler>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..ferrite_data::block_props::STATE_COUNT)
            .filter_map(|id| {
                let id = u16::try_from(id).ok()?;
                ferrite_data::block_props::get(id)
                    .randomly_ticking
                    .then_some((u32::from(id), no_op as Handler))
            })
            .collect()
    })
}
fn sample_chunk(
    chunk: &Chunk,
    cx: i32,
    cz: i32,
    speed: u32,
    mut next: impl FnMut() -> usize,
    mut emit: impl FnMut(Event),
) {
    use ferrite_data::block_props::FluidKind;
    for (section_y, section) in chunk.sections().iter().enumerate() {
        if !section.is_randomly_ticking() {
            continue;
        }
        for _ in 0..speed {
            let index = next();
            let Some(state) = section.blocks().get(index) else {
                continue;
            };
            let Ok(id) = u16::try_from(state) else {
                continue;
            };
            if usize::from(id) >= ferrite_data::block_props::STATE_COUNT {
                continue;
            }
            let props = ferrite_data::block_props::get(id);
            let fluid_tick = matches!(
                props.fluid,
                Some((FluidKind::Lava | FluidKind::FlowingLava, _, _))
            );
            if props.randomly_ticking || fluid_tick {
                emit(Event {
                    pos: (
                        cx * 16 + (index & 15) as i32,
                        chunk.min_y() + section_y as i32 * 16 + ((index >> 8) & 15) as i32,
                        cz * 16 + ((index >> 4) & 15) as i32,
                    ),
                    state,
                    block_tick: props.randomly_ticking,
                    fluid_tick,
                });
            }
        }
    }
}
pub(super) struct RandomTicks {
    rng: XoroshiroRandomSource,
}
impl Default for RandomTicks {
    fn default() -> Self {
        static SEED: AtomicU64 = AtomicU64::new(0x91e10da5c79e7b1d);
        Self {
            rng: XoroshiroRandomSource::new(
                SEED.fetch_add(0x9e3779b97f4a7c15, Ordering::Relaxed) as i64
            ),
        }
    }
}
impl RandomTicks {
    pub fn tick(
        &mut self,
        source: &AnvilChunkSource,
        held: &HashMap<ChunkPos, usize>,
        tracking: &Tracking,
        speed: u32,
    ) {
        let mut world = RegionFluidWorld::new(source, held, tracking);
        for chunk in held.keys() {
            let mut events = Vec::new();
            source.with_loaded_chunk(chunk.0, chunk.1, |data| {
                sample_chunk(
                    data,
                    chunk.0,
                    chunk.1,
                    speed,
                    || self.rng.next_int_bound(4096) as usize,
                    |event| events.push(event),
                )
            });
            for event in events {
                if world.block(event.pos.0, event.pos.1, event.pos.2) != event.state {
                    continue;
                }
                if event.block_tick
                    && let Some(handler) = handlers().get(&event.state)
                {
                    handler(&mut world, event.pos, event.state, &mut self.rng);
                }
                // LavaFluid.randomTick only starts fire, which is out of scope.
                if event.fluid_tick {
                    no_op(&mut world, event.pos, event.state, &mut self.rng);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_world::{BlockFlags, BlockInfo, Chunk};
    struct Info;
    impl BlockInfo for Info {
        fn flags(&self, id: u32) -> BlockFlags {
            BlockFlags {
                air: id == 0,
                ..Default::default()
            }
        }
        fn randomly_ticking(&self, id: u32) -> bool {
            id == u32::from(ferrite_data::blocks::default_state("grass_block").unwrap())
        }
    }
    #[test]
    fn non_ticking_sections_do_not_sample_and_ticking_sections_use_one_draw_per_pick() {
        let mut chunk = Chunk::new(-64, 32, 0, 0, 16, 6).unwrap();
        let grass = u32::from(ferrite_data::blocks::default_state("grass_block").unwrap());
        chunk.sections_mut()[1].set_block(0, grass, &Info).unwrap();
        let mut draws = 0;
        let mut events = Vec::new();
        sample_chunk(
            &chunk,
            2,
            -1,
            3,
            || {
                draws += 1;
                0
            },
            |e| events.push(e),
        );
        assert_eq!(draws, 3);
        assert_eq!(events.len(), 3);
        assert!(
            events
                .iter()
                .all(|e| e.pos == (32, -48, -16) && e.state == grass && e.block_tick)
        );
    }
}
