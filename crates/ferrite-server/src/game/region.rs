use super::{
    PluginEventPolicy,
    chunk::{ChunkSource, FlatGenerator},
    commands::{XpOperation, XpUnit},
    valid_move,
};
use ferrite_entity::{
    BlockGetter, EntityRecord, EntityStore, Rng, item,
    tracker::{self, Viewer, ViewerKey},
};
use ferrite_net::game::{Outbound, OutboundSender, ServerboundPlay, packet};
use ferrite_plugin::InteractAction;
use ferrite_profiler::Timings;
use ferrite_protocol::{VarInt, packets::play::*};
use ferrite_region::{ChunkPos, RegionData, SectionPos};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;
pub enum Command {
    Input(ServerboundPlay),
    Leave,
    SetMode(i32),
    Teleport {
        pos: [f64; 3],
        yaw: Option<f32>,
        pitch: Option<f32>,
    },
    World(super::commands_world::WorldCommand),
    Clear {
        target: Uuid,
        target_name: String,
        item: Option<u16>,
        max_count: Option<i32>,
        feedback: Option<OutboundSender>,
    },
    Give {
        target: Uuid,
        target_name: String,
        item: u16,
        count: i32,
        item_name: String,
        feedback: Option<OutboundSender>,
    },
    Experience {
        target: Uuid,
        target_name: String,
        operation: XpOperation,
        amount: i32,
        unit: XpUnit,
        feedback: Option<OutboundSender>,
    },
}
pub type Mailbox = Arc<Mutex<VecDeque<Command>>>;
pub type Tracking = Arc<Mutex<HashMap<Uuid, (HashSet<(i32, i32)>, OutboundSender)>>>;
pub enum Signal {
    Transfer {
        from: ChunkPos,
        to: ChunkPos,
        player: Box<Player>,
    },
    Hold {
        chunk: ChunkPos,
        add: bool,
    },
    FluidChange {
        x: i32,
        y: i32,
        z: i32,
    },
    FinishRelease {
        chunk: ChunkPos,
        flush: bool,
    },
    Update {
        uuid: Uuid,
        pos: [f64; 3],
        yaw: f32,
        pitch: f32,
        ground: bool,
    },
    Leave {
        uuid: Uuid,
        chunk: ChunkPos,
        held: Vec<ChunkPos>,
    },
    /// An entity walked into another region section; the main loop hands it to the region
    /// owning `to` through that region's task queue (Folia: only the owner touches it).
    EntityTransfer {
        to: ChunkPos,
        entity: Box<EntityRecord>,
    },
}
/// What a region tick needs from the world loop.
#[derive(Clone)]
pub struct TickContext {
    pub signals: UnboundedSender<Signal>,
    /// Every online player's sent chunks and connection; reaches players that left the region.
    pub tracking: Tracking,
    /// World blocks, for entity physics.
    pub source: Option<Arc<super::anvil::AnvilChunkSource>>,
}
/// Block access for entity physics, memoized for one tick (each lookup locks the chunk cache).
struct TickBlocks<'a> {
    source: Option<&'a super::anvil::AnvilChunkSource>,
    seen: std::cell::RefCell<HashMap<(i32, i32, i32), Option<u16>>>,
}
/// Physics reads through the tick's memoized block getter; placements write through the
/// region's fluid world, so tracking players get the `block_update`.
struct FallingAdapter<'a> {
    read: &'a TickBlocks<'a>,
    world: super::fluid::RegionFluidWorld<'a>,
}

impl BlockGetter for FallingAdapter<'_> {
    fn block_state(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        self.read.block_state(x, y, z)
    }
}

impl ferrite_entity::falling::FallingWorld for FallingAdapter<'_> {
    fn set_block(&mut self, x: i32, y: i32, z: i32, state: u16) {
        ferrite_fluid::FluidWorld::set_block(&mut self.world, x, y, z, u32::from(state));
    }
}

impl BlockGetter for TickBlocks<'_> {
    fn block_state(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        let source = self.source?;
        *self
            .seen
            .borrow_mut()
            .entry((x, y, z))
            .or_insert_with(|| source.block(x, y, z).and_then(|s| u16::try_from(s).ok()))
    }
}
/// How many queued chunks to request ahead of the batch being sent.
const PREFETCH_CHUNKS: usize = 96;

pub struct Player {
    pub uuid: Uuid,
    /// Login session; a reconnect gets a new one, so trackers treat it as a new viewer.
    pub session: u64,
    /// Network entity id.
    pub entity: i32,
    /// Blocks this player broke in survival this tick, with their previous state.
    pub broken: Vec<(ferrite_protocol::types::BlockPos, u32)>,
    /// Blocks this player placed or broke this tick, for neighbor updates.
    pub changed_blocks: Vec<[i32; 3]>,
    /// TNT this player primed this tick, with the fuse length; spawned as `PrimedTnt`.
    pub primed_tnt: Vec<([i32; 3], i32)>,
    pub name: String,
    /// Plugin event hooks; `None` when the server runs without plugins.
    pub plugins: Option<Arc<dyn ferrite_plugin::ServerHooks>>,
    /// What a failed plugin event means for the action it guards.
    pub plugin_event_errors: PluginEventPolicy,
    pub positions: Arc<Mutex<HashMap<Uuid, [f64; 3]>>>,
    pub tracking: Tracking,
    pub source: Option<Arc<super::anvil::AnvilChunkSource>>,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub ground: bool,
    pub chunk: ChunkPos,
    pub mailbox: Mailbox,
    pub outbound: OutboundSender,
    pub signals: UnboundedSender<Signal>,
    pub biome: i32,
    pub gamemode: i32,
    pub experience_level: i32,
    pub experience_progress: f32,
    pub experience_total: i32,
    pub inventory: ferrite_inventory::PlayerInventory,
    pub menus: super::container::Containers,
    pub breaking: Option<(i64, u32, u32)>,
    pub view: i32,
    pub pending_teleport: Option<i32>,
    pub next_teleport: i32,
    pub loaded: HashSet<(i32, i32)>,
    pub queue: VecDeque<(i32, i32)>,
    pub awaiting_batch: bool,
    pub rate: f32,
}
#[derive(Default)]
pub struct Region {
    pub players: HashMap<Uuid, Player>,
    pub entities: EntityStore,
    generator: FlatGenerator,
    fluid_ticks: ferrite_fluid::FluidTicks,
    random_ticks: super::random_tick::RandomTicks,
    updates: super::neighbors::Updates,
    gravity: super::gravity::Gravity,
    held_chunks: HashMap<ChunkPos, usize>,
    /// Tick monitor slot, assigned by the game loop on this region's first tick.
    pub profile_slot: Option<u32>,
    /// Scope tree attached by the game loop; `None` until the first tick.
    pub timings: Option<Arc<Timings>>,
    rng: Rng,
}
impl RegionData for Region {
    fn merge(&mut self, mut other: Self) {
        other
            .fluid_ticks
            .transfer_to(&mut self.fluid_ticks, |_| true);
        for (chunk, count) in other.held_chunks {
            *self.held_chunks.entry(chunk).or_default() += count;
        }
        self.gravity.merge(other.gravity);
        self.players.extend(other.players);
        self.entities.merge(other.entities);
        self.generator.merge(other.generator);
    }
    fn split(&mut self, sections: &HashSet<SectionPos>) -> Self {
        let ids: Vec<_> = self
            .players
            .iter()
            .filter(|(_, p)| sections.contains(&SectionPos(p.chunk.0 >> 4, p.chunk.1 >> 4)))
            .map(|(id, _)| *id)
            .collect();
        let mut result = Self {
            players: HashMap::new(),
            rng: Rng::default(),
            entities: self
                .entities
                .split(|(x, z)| sections.contains(&SectionPos(x >> 4, z >> 4))),
            generator: self.generator.split(sections),
            fluid_ticks: ferrite_fluid::FluidTicks::default(),
            random_ticks: super::random_tick::RandomTicks::default(),
            updates: super::neighbors::Updates::default(),
            gravity: self.gravity.split(|x, z| {
                sections.contains(&SectionPos(x.div_euclid(16) >> 4, z.div_euclid(16) >> 4))
            }),
            held_chunks: HashMap::new(),
            profile_slot: None,
            timings: self.timings.clone(),
        };
        let moving: Vec<_> = self
            .held_chunks
            .keys()
            .copied()
            .filter(|c| sections.contains(&SectionPos(c.0 >> 4, c.1 >> 4)))
            .collect();
        for c in moving {
            result
                .held_chunks
                .insert(c, self.held_chunks.remove(&c).unwrap());
        }
        self.fluid_ticks
            .transfer_to(&mut result.fluid_ticks, |(x, _, z)| {
                sections.contains(&SectionPos(
                    (x.div_euclid(16)) >> 4,
                    (z.div_euclid(16)) >> 4,
                ))
            });
        for id in ids {
            if let Some(p) = self.players.remove(&id) {
                result.players.insert(id, p);
            }
        }
        result
    }
}
type ChunkList = Vec<(i32, i32)>;
fn chunk_window(
    center: (i32, i32),
    radius: i32,
    loaded: &HashSet<(i32, i32)>,
) -> (ChunkList, ChunkList) {
    let forget = loaded
        .iter()
        .copied()
        .filter(|(x, z)| (x - center.0).abs() > radius || (z - center.1).abs() > radius)
        .collect();
    let mut new = Vec::new();
    for x in center.0 - radius..=center.0 + radius {
        for z in center.1 - radius..=center.1 + radius {
            if !loaded.contains(&(x, z)) {
                new.push((x, z));
            }
        }
    }
    new.sort_by_key(|(x, z)| {
        let dx = x - center.0;
        let dz = z - center.1;
        (dx * dx + dz * dz, *x, *z)
    });
    (forget, new)
}
pub(super) fn send<P: ferrite_protocol::Packet>(p: &Player, msg: &P) {
    let _ = p.outbound.send(packet(msg));
}
/// Namespaced block id of a global state id, for Bukkit event materials.
fn material_name(state: u32) -> &'static str {
    u16::try_from(state)
        .ok()
        .and_then(ferrite_data::blocks::state)
        .map_or("minecraft:air", |(name, _)| name)
}
/// Runs an event hook. A missing bridge allows the action; a failed call is
/// decided by `plugin-event-errors` (Bukkit-compatible default: allow).
fn event_allows(
    plugins: Option<&dyn ferrite_plugin::ServerHooks>,
    policy: PluginEventPolicy,
    event: impl FnOnce(&dyn ferrite_plugin::ServerHooks) -> Result<bool, ferrite_plugin::Error>,
) -> bool {
    let Some(plugins) = plugins else {
        return true;
    };
    match event(plugins) {
        Ok(allowed) => allowed,
        Err(error) => {
            let allow = policy == PluginEventPolicy::Allow;
            tracing::warn!(%error, allow, "plugin event failed");
            allow
        }
    }
}
/// Vanilla's total experience required to reach `level`.
fn total_for_level(level: i32) -> i64 {
    let level = i64::from(level.max(0));
    if level <= 16 {
        level * level + 6 * level
    } else if level <= 31 {
        (2.5 * level as f64 * level as f64 - 40.5 * level as f64 + 360.0) as i64
    } else {
        (4.5 * level as f64 * level as f64 - 162.5 * level as f64 + 2220.0) as i64
    }
}

/// Level and bar progress for a total amount of experience.
fn level_and_progress(total: i32) -> (i32, f32) {
    let total = i64::from(total.max(0));
    let mut level = 0;
    while level < 21_863 && total >= total_for_level(level + 1) {
        level += 1;
    }
    let base = total_for_level(level);
    let next = total_for_level(level + 1);
    let progress = if next > base {
        ((total - base) as f32 / (next - base) as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (level, progress)
}
impl Player {
    pub fn refresh_chunks(&mut self) {
        send(
            self,
            &CenterChunk {
                x: VarInt(self.chunk.0),
                z: VarInt(self.chunk.1),
            },
        );
        let (forget, new) = chunk_window((self.chunk.0, self.chunk.1), self.view, &self.loaded);
        for (x, z) in forget {
            send(self, &ForgetChunk { z, x });
            self.loaded.remove(&(x, z));
            if let Some((loaded, _)) = self.tracking.lock().unwrap().get_mut(&self.uuid) {
                loaded.remove(&(x, z));
            }
            let _ = self.signals.send(Signal::Hold {
                chunk: ChunkPos(x, z),
                add: false,
            });
        }
        self.queue = new.into();
    }
    fn teleport(&mut self) {
        let id = self.next_teleport;
        self.next_teleport += 1;
        self.pending_teleport = Some(id);
        send(
            self,
            &PositionSync {
                teleport_id: VarInt(id),
                x: self.pos[0],
                y: self.pos[1],
                z: self.pos[2],
                velocity_x: 0.0,
                velocity_y: 0.0,
                velocity_z: 0.0,
                yaw: self.yaw,
                pitch: self.pitch,
                flags: 0,
            },
        );
    }
    fn chunks_tick(&mut self, generator: &mut FlatGenerator) {
        if self.awaiting_batch || self.queue.is_empty() {
            return;
        }
        // Start loading the whole head of the queue at once; `get` hands back chunks that are ready
        // and spawns background loads for the rest. Without this, chunks became ready one per tick.
        if let Some(source) = &self.source {
            for &(x, z) in self.queue.iter().take(PREFETCH_CHUNKS) {
                source.prefetch(x, z, self.biome);
            }
        }
        let mut tracking = self.tracking.lock().unwrap();
        let count = (self.rate.clamp(1.0, 64.0) as usize).min(self.queue.len());
        let mut chunks = Vec::new();
        for _ in 0..count {
            let Some(&(x, z)) = self.queue.front() else {
                break;
            };
            let packet = if let Some(source) = &self.source {
                match source.get(x, z, self.biome) {
                    None => break,
                    // The cache shares one encoded body; the connection layer
                    // needs an owned buffer, so this is the only copy per send.
                    Some(Ok(packet)) => packet.to_vec(),
                    Some(Err(error)) => {
                        tracing::error!(x, z, %error, "failed to load chunk");
                        let _ = self
                            .outbound
                            .send(Outbound::Disconnect("Failed to load world chunk".into()));
                        self.mailbox.lock().unwrap().push_back(Command::Leave);
                        return;
                    }
                }
            } else {
                let chunk = generator.get(x, z, self.biome);
                let mut packet = Vec::new();
                ferrite_protocol::encode_packet(&chunk, &mut packet);
                packet
            };
            self.queue.pop_front();
            chunks.push((x, z, packet));
        }
        if chunks.is_empty() {
            return;
        }
        send(self, &ChunkBatchStart);
        let count = chunks.len();
        for (x, z, packet) in chunks {
            let _ = self.outbound.send(Outbound::Packet(packet.into()));
            self.loaded.insert((x, z));
            if let Some((loaded, _)) = tracking.get_mut(&self.uuid) {
                loaded.insert((x, z));
            }
            let _ = self.signals.send(Signal::Hold {
                chunk: ChunkPos(x, z),
                add: true,
            });
        }
        send(
            self,
            &ChunkBatchFinished {
                size: VarInt(count as i32),
            },
        );
        self.awaiting_batch = true;
    }
    fn interaction_ready(&self, location: i64) -> bool {
        let pos = ferrite_protocol::types::BlockPos::unpack(location);
        if (f64::from(pos.x) - self.pos[0]).abs() > 7.0
            || (f64::from(pos.y) - self.pos[1]).abs() > 7.0
            || (f64::from(pos.z) - self.pos[2]).abs() > 7.0
        {
            return true;
        }
        let Some(source) = &self.source else {
            return true;
        };
        let mut ready = true;
        let min_x = (self.pos[0] - 6.0).floor() as i32;
        let max_x = (self.pos[0] + 6.0).floor() as i32;
        let min_z = (self.pos[2] - 6.0).floor() as i32;
        let max_z = (self.pos[2] + 6.0).floor() as i32;
        for x in min_x.div_euclid(16)..=max_x.div_euclid(16) {
            for z in min_z.div_euclid(16)..=max_z.div_euclid(16) {
                match source.get(x, z, self.biome) {
                    None => ready = false,
                    Some(Err(error)) => {
                        tracing::error!(x,z,%error,"failed to load interaction chunk");
                        return true;
                    }
                    Some(Ok(_)) => {}
                }
            }
        }
        ready
    }
    fn hit(&self, target: ferrite_protocol::types::BlockPos) -> Option<ferrite_math::BlockHit> {
        use ferrite_math::{AABB, Vec3, look_from_yaw_pitch, raycast};
        let source = self.source.as_ref()?;
        let eye = Vec3::new(self.pos[0], self.pos[1] + 1.62, self.pos[2]);
        let mut shapes = HashMap::new();
        let look = look_from_yaw_pitch(self.yaw, self.pitch);
        let distance = if self.gamemode == 1 { 5.0 } else { 4.5 };
        let end = eye + look * distance;
        let start = ferrite_math::BlockPos::containing(eye);
        let end = ferrite_math::BlockPos::containing(end);
        for x in start.x.min(end.x)..=start.x.max(end.x) {
            for y in start.y.min(end.y)..=start.y.max(end.y) {
                for z in start.z.min(end.z)..=start.z.max(end.z) {
                    let boxes: Vec<_> = match source
                        .block(x, y, z)
                        .and_then(|v| u16::try_from(v).ok())
                        .filter(|v| usize::from(*v) < ferrite_data::blocks::STATE_COUNT)
                    {
                        Some(state) => ferrite_data::block_props::collision_shape(state)
                            .iter()
                            .map(|b| AABB::new(b[0], b[1], b[2], b[3], b[4], b[5]))
                            .collect(),
                        None => vec![AABB::new(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)],
                    };
                    if !boxes.is_empty() {
                        shapes.insert((x, y, z), boxes);
                    }
                }
            }
        }
        raycast(eye, look, distance, |p| {
            shapes.get(&(p.x, p.y, p.z)).map(Vec::as_slice)
        })
        .filter(|hit| (hit.pos.x, hit.pos.y, hit.pos.z) == (target.x, target.y, target.z))
    }
    fn send_block_snapshot(&self, location: i64) {
        let pos = ferrite_protocol::types::BlockPos::unpack(location);
        if let Some(state) = self
            .source
            .as_ref()
            .and_then(|s| s.block(pos.x, pos.y, pos.z))
        {
            send(
                self,
                &BlockUpdate {
                    location,
                    block_state: VarInt(state as i32),
                },
            );
        }
    }
    /// Applies a block change and tells every player tracking the chunk. `expected` keeps the
    /// compare-and-set used by player interaction; commands pass `None`.
    pub(crate) fn change_block(&self, location: i64, expected: Option<u32>, state: u32) -> bool {
        let pos = ferrite_protocol::types::BlockPos::unpack(location);
        let tracking = self.tracking.lock().unwrap();
        if self.source.as_ref().is_some_and(|source| {
            source
                .compare_set_block(pos.x, pos.y, pos.z, expected, state)
                .is_ok()
        }) {
            let chunk = (pos.x.div_euclid(16), pos.z.div_euclid(16));
            // One encoding, shared by every observer.
            let update = packet(&BlockUpdate {
                location,
                block_state: VarInt(state as i32),
            });
            for (loaded, outbound) in tracking.values() {
                if loaded.contains(&chunk) {
                    let _ = outbound.send(update.clone());
                }
            }
            let _ = self.signals.send(Signal::FluidChange {
                x: pos.x,
                y: pos.y,
                z: pos.z,
            });
            true
        } else {
            false
        }
    }
    /// Breaks a block (to air); an inventory block drops its contents (vanilla
    /// `Containers.dropContentsOnDestroy`).
    fn break_block(&mut self, location: i64, state: u32) -> bool {
        if !self.change_block(location, Some(state), 0) {
            return false;
        }
        let pos = ferrite_protocol::types::BlockPos::unpack(location);
        let pos = [pos.x, pos.y, pos.z];
        self.changed_blocks.push(pos);
        if let Some(source) = &self.source {
            for stack in super::block_container::take_contents(source, pos, state) {
                self.menus.spilled.push((pos, stack));
            }
        }
        true
    }
    fn place(&mut self, p: &UseItemOn) {
        if !(self.gamemode == 0 || self.gamemode == 1)
            || !(0..=1).contains(&p.hand.0)
            || !(0..=5).contains(&p.face.0)
            || ![p.cursor_x, p.cursor_y, p.cursor_z]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            || p.world_border_hit
        {
            return;
        }
        let clicked = ferrite_protocol::types::BlockPos::unpack(p.location);
        if !self
            .hit(clicked)
            .is_some_and(|hit| hit.face == ferrite_math::Direction::ALL[p.face.0 as usize])
        {
            return;
        }
        let Some(source) = &self.source else { return };
        let Some(clicked_state) = source
            .block(clicked.x, clicked.y, clicked.z)
            .and_then(|s| u16::try_from(s).ok())
        else {
            return;
        };
        // Vanilla `FlintAndSteelItem.useOn` on TNT: prime it instead of placing the held item.
        let held = self.inventory.get(self.hand_slot(p.hand.0)).clone();
        if held.name() == Some("minecraft:flint_and_steel")
            && ferrite_data::blocks::state(clicked_state)
                .is_some_and(|(name, _)| name == "minecraft:tnt")
        {
            if self.change_block(p.location, Some(u32::from(clicked_state)), 0) {
                self.changed_blocks.push([clicked.x, clicked.y, clicked.z]);
                self.primed_tnt
                    .push(([clicked.x, clicked.y, clicked.z], ferrite_entity::tnt::FUSE));
            }
            return;
        }
        let face = ferrite_math::Direction::ALL[p.face.0 as usize];
        let (dx, dy, dz) = if ferrite_data::block_props::get(clicked_state).replaceable {
            (0, 0, 0)
        } else {
            face.step()
        };
        let target =
            ferrite_protocol::types::BlockPos::new(clicked.x + dx, clicked.y + dy, clicked.z + dz);
        let Some(old) = source
            .block(target.x, target.y, target.z)
            .and_then(|s| u16::try_from(s).ok())
        else {
            return;
        };
        if !ferrite_data::block_props::get(old).replaceable {
            return;
        }
        let slot = self.hand_slot(p.hand.0);
        let item = self.inventory.get(slot).clone();
        if item.is_empty() {
            return;
        }
        let item_id = i32::from(item.item);
        let water_bucket = item.name() == Some("minecraft:water_bucket");
        let state = if water_bucket {
            ferrite_data::blocks::default_state("water").expect("vanilla water")
        } else {
            let Some(state) =
                super::interaction::placement_state(item_id, p.face.0, p.cursor_y, self.yaw, old)
            else {
                return;
            };
            state
        };
        // Vanilla `BlockItem.place` -> `BlockPlaceContext.canPlace` -> `canSurvive`.
        if !super::neighbors::can_survive(state, [target.x, target.y, target.z], source.as_ref()) {
            self.send_block_snapshot(target.pack());
            return;
        }
        if !self.plugin_allows(|plugins| {
            plugins.block_placed(
                self.uuid,
                &self.name,
                ferrite_plugin::BlockPosition {
                    x: target.x,
                    y: target.y,
                    z: target.z,
                },
                u8::try_from(p.face.0.clamp(0, 5)).unwrap_or(0),
                material_name(u32::from(state)),
                material_name(u32::from(old)),
            )
        }) {
            self.send_block_snapshot(target.pack());
            return;
        }
        let positions = self.positions.lock().unwrap();
        let offset = ferrite_math::Vec3::new(target.x as f64, target.y as f64, target.z as f64);
        if positions.iter().any(|(uuid, pos)| {
            let pos = if *uuid == self.uuid { &self.pos } else { pos };
            let player_box = ferrite_math::AABB::new(
                pos[0] - 0.3,
                pos[1],
                pos[2] - 0.3,
                pos[0] + 0.3,
                pos[1] + 1.8,
                pos[2] + 0.3,
            );
            ferrite_data::block_props::collision_shape(state)
                .iter()
                .any(|b| {
                    player_box.intersects(
                        ferrite_math::AABB::new(b[0], b[1], b[2], b[3], b[4], b[5]).move_by(offset),
                    )
                })
        }) {
            return;
        }
        let placed = self.change_block(target.pack(), Some(u32::from(old)), u32::from(state));
        drop(positions);
        if placed {
            self.changed_blocks.push([target.x, target.y, target.z]);
        }
        // Survival uses the item up (vanilla `BlockItem.place`: `shrink(1)` unless the player
        // has infinite materials); the client gets the slot with the tick's `broadcastChanges`.
        if placed && self.gamemode == 0 {
            if water_bucket {
                self.inventory.set(
                    slot,
                    ferrite_item::ItemStack::from_name("bucket").expect("vanilla bucket"),
                );
            } else {
                self.inventory
                    .set(slot, item.copy_with_count(item.count - 1));
            }
        }
    }
    /// Runs an event hook; a missing bridge or a failed call is decided by the
    /// configured [`PluginEventPolicy`]. Callers cancel the action on `false`,
    /// which resends the original block or teleport state.
    fn plugin_allows(
        &self,
        event: impl FnOnce(&dyn ferrite_plugin::ServerHooks) -> Result<bool, ferrite_plugin::Error>,
    ) -> bool {
        event_allows(self.plugins.as_deref(), self.plugin_event_errors, event)
    }

    fn feedback(&self, feedback: Option<&OutboundSender>, text: String) {
        match feedback {
            Some(tx) => {
                let _ = tx.send(packet(&SystemChat {
                    content: ferrite_protocol::types::PlainText(text),
                    overlay: false,
                }));
            }
            None => tracing::info!("{text}"),
        }
    }
    /// `/clear`: removes up to `max_count` matching items (all when `None`).
    fn clear_items(
        &mut self,
        item: Option<u16>,
        max_count: Option<i32>,
        target_name: &str,
        feedback: Option<&OutboundSender>,
    ) {
        let matches = |stack: &ferrite_item::ItemStack| {
            !stack.is_empty() && item.is_none_or(|id| stack.item == id)
        };
        if max_count == Some(0) {
            let found: i32 = (0..ferrite_inventory::SIZE)
                .map(|slot| self.inventory.get(slot))
                .filter(|stack| matches(stack))
                .map(|stack| stack.count)
                .sum();
            let text = if found == 0 {
                format!("No items were found on player {target_name}")
            } else {
                format!("Found {found} matching item(s) on player {target_name}")
            };
            self.feedback(feedback, text);
            return;
        }
        let limit = max_count.unwrap_or(i32::MAX);
        let mut removed = 0;
        for slot in 0..ferrite_inventory::SIZE {
            if removed >= limit {
                break;
            }
            if !matches(self.inventory.get(slot)) {
                continue;
            }
            let take = self.inventory.get(slot).count.min(limit - removed);
            removed += self.inventory.remove(slot, take).count;
        }
        if removed == 0 {
            self.feedback(
                feedback,
                format!("No items were found on player {target_name}"),
            );
            return;
        }
        self.sync_containers();
        self.feedback(
            feedback,
            format!("Removed {removed} items from {target_name}"),
        );
    }
    /// `/give`: places the stack back in the inventory, dropping what does not fit.
    fn give_items(
        &mut self,
        item: u16,
        count: i32,
        item_name: &str,
        target_name: &str,
        feedback: Option<&OutboundSender>,
    ) {
        let leftover = self
            .inventory
            .place_back(ferrite_item::ItemStack::with_count(item, count));
        let given = count - leftover.as_ref().map_or(0, |stack| stack.count);
        if let Some(leftover) = leftover {
            self.menus.thrown.push(leftover);
        }
        if given > 0 {
            self.sync_containers();
        }
        self.feedback(
            feedback,
            format!("Gave {given} {item_name} to {target_name}"),
        );
    }
    fn set_experience_total(&mut self, total: i64) {
        let total = total.clamp(0, i64::from(i32::MAX)) as i32;
        let (level, progress) = level_and_progress(total);
        self.experience_total = total;
        self.experience_level = level;
        self.experience_progress = progress;
    }
    fn set_experience_level(&mut self, level: i32) {
        let level = level.clamp(0, 21_863);
        self.experience_level = level;
        self.experience_progress = 0.0;
        self.experience_total = total_for_level(level).min(i64::from(i32::MAX)) as i32;
    }
    /// `/xp`: add, set or query experience points/levels.
    fn apply_experience(
        &mut self,
        operation: XpOperation,
        amount: i32,
        unit: XpUnit,
        target_name: &str,
        feedback: Option<&OutboundSender>,
    ) {
        if operation == XpOperation::Query {
            let text = match unit {
                XpUnit::Points => {
                    format!("{target_name} has {} experience", self.experience_total)
                }
                XpUnit::Levels => format!(
                    "{target_name} has {} experience levels",
                    self.experience_level
                ),
            };
            self.feedback(feedback, text);
            return;
        }
        match (operation, unit) {
            (XpOperation::Add, XpUnit::Points) => {
                self.set_experience_total(i64::from(self.experience_total) + i64::from(amount));
            }
            (XpOperation::Add, XpUnit::Levels) => {
                self.set_experience_level(self.experience_level.saturating_add(amount));
            }
            (XpOperation::Set, XpUnit::Points) => self.set_experience_total(i64::from(amount)),
            (XpOperation::Set, XpUnit::Levels) => self.set_experience_level(amount),
            (XpOperation::Query, _) => unreachable!(),
        }
        send(
            self,
            &SetExperience {
                progress: self.experience_progress,
                level: VarInt(self.experience_level),
                total: VarInt(self.experience_total),
            },
        );
        let text = match operation {
            XpOperation::Add if amount < 0 => match unit {
                XpUnit::Points => format!(
                    "Took {} experience from {target_name}",
                    amount.unsigned_abs()
                ),
                XpUnit::Levels => {
                    format!("Took {} levels from {target_name}", amount.unsigned_abs())
                }
            },
            XpOperation::Add => match unit {
                XpUnit::Points => format!("Gave {amount} experience to {target_name}"),
                XpUnit::Levels => format!("Gave {amount} levels to {target_name}"),
            },
            XpOperation::Set => match unit {
                XpUnit::Points => {
                    format!("Set {target_name}'s experience to {amount}")
                }
                XpUnit::Levels => {
                    format!("Set {target_name}'s experience level to {amount}")
                }
            },
            XpOperation::Query => unreachable!(),
        };
        self.feedback(feedback, text);
    }
    fn update(&mut self) -> bool {
        if let Some((_, _, elapsed)) = &mut self.breaking {
            *elapsed = elapsed.saturating_add(1);
        }
        let mut commands: VecDeque<_> = self.mailbox.lock().unwrap().drain(..).collect();
        let start = self.pos;
        let mut changed = false;
        while let Some(command) = commands.pop_front() {
            let location = match &command {
                Command::Input(ServerboundPlay::PlayerAction(p))
                    if p.status.0 == 0 || p.status.0 == 2 =>
                {
                    Some(p.location)
                }
                Command::Input(ServerboundPlay::UseItemOn(p)) => Some(p.location),
                _ => None,
            };
            if location.is_some_and(|location| !self.interaction_ready(location)) {
                let mut mailbox = self.mailbox.lock().unwrap();
                commands.push_front(command);
                commands.append(&mut mailbox);
                *mailbox = commands;
                break;
            }
            let msg = match command {
                Command::Leave => {
                    self.close_container();
                    super::playerdata::save(
                        &self.uuid,
                        &super::playerdata::Saved::capture(
                            self.pos,
                            self.yaw,
                            self.pitch,
                            self.gamemode,
                            &self.inventory,
                        ),
                    );
                    return false;
                }
                Command::SetMode(mode) => {
                    self.gamemode = mode;
                    send(
                        self,
                        &GameEvent {
                            event: 3,
                            value: mode as f32,
                        },
                    );
                    continue;
                }
                Command::Teleport { pos, yaw, pitch } => {
                    if pos.iter().all(|v| v.is_finite() && v.abs() <= 30_000_000.0) {
                        self.pos = pos;
                        if let Some(yaw) = yaw {
                            self.yaw = yaw;
                        }
                        if let Some(pitch) = pitch {
                            self.pitch = pitch;
                        }
                        self.teleport();
                        changed = true;
                    }
                    continue;
                }
                Command::World(command) => {
                    super::commands_world::run(self, command);
                    continue;
                }
                Command::Clear {
                    target,
                    target_name,
                    item,
                    max_count,
                    feedback,
                } => {
                    if target == self.uuid {
                        self.clear_items(item, max_count, &target_name, feedback.as_ref());
                    }
                    continue;
                }
                Command::Give {
                    target,
                    target_name,
                    item,
                    count,
                    item_name,
                    feedback,
                } => {
                    if target == self.uuid {
                        self.give_items(item, count, &item_name, &target_name, feedback.as_ref());
                    }
                    continue;
                }
                Command::Experience {
                    target,
                    target_name,
                    operation,
                    amount,
                    unit,
                    feedback,
                } => {
                    if target == self.uuid {
                        self.apply_experience(
                            operation,
                            amount,
                            unit,
                            &target_name,
                            feedback.as_ref(),
                        );
                    }
                    continue;
                }
                Command::Input(msg) => msg,
            };
            match msg {
                ServerboundPlay::PlayerAction(p) if matches!(p.status.0, 3 | 4 | 6) => {
                    match p.status.0 {
                        3 => self.drop_selected(true),
                        4 => self.drop_selected(false),
                        _ => self.swap_hands(),
                    }
                }
                ServerboundPlay::ContainerClick(p) => self.handle_container_click(p),
                ServerboundPlay::ContainerClose(_) => self.close_container(),
                ServerboundPlay::PlayerAction(p) => {
                    let pos = ferrite_protocol::types::BlockPos::unpack(p.location);
                    match p.status.0 {
                        0 if self.hit(pos).is_some() => {
                            let clicked = ferrite_plugin::BlockPosition {
                                x: pos.x,
                                y: pos.y,
                                z: pos.z,
                            };
                            let state = self
                                .source
                                .as_ref()
                                .and_then(|s| s.block(pos.x, pos.y, pos.z));
                            let allowed = self.plugin_allows(|plugins| {
                                plugins.player_interacted(
                                    self.uuid,
                                    &self.name,
                                    InteractAction::LeftClickBlock,
                                    clicked,
                                    u8::try_from(p.face.clamp(0, 5)).unwrap_or(0),
                                    material_name(state.unwrap_or(0)),
                                )
                            });
                            if allowed && let Some(state) = state {
                                if self.gamemode == 1 {
                                    if self.plugin_allows(|plugins| {
                                        plugins.block_broken(
                                            self.uuid,
                                            &self.name,
                                            clicked,
                                            material_name(state),
                                        )
                                    }) {
                                        self.break_block(p.location, state);
                                    }
                                } else if self.gamemode == 0 {
                                    // Zero hardness breaks on the first hit (vanilla: destroy
                                    // progress >= 1 on START), so no finish action follows.
                                    let instant = u16::try_from(state).is_ok_and(|s| {
                                        usize::from(s) < ferrite_data::blocks::STATE_COUNT
                                            && ferrite_data::block_props::get(s).hardness == 0.0
                                    });
                                    if instant {
                                        if self.plugin_allows(|plugins| {
                                            plugins.block_broken(
                                                self.uuid,
                                                &self.name,
                                                clicked,
                                                material_name(state),
                                            )
                                        }) && self.break_block(p.location, state)
                                        {
                                            self.broken.push((pos, state));
                                        }
                                    } else {
                                        self.breaking = Some((p.location, state, 0));
                                    }
                                }
                            }
                        }
                        1 => self.breaking = None,
                        2 => {
                            if let Some((location, state, elapsed)) = self.breaking.take()
                                && location == p.location
                                && self.hit(pos).is_some()
                                && u16::try_from(state)
                                    .ok()
                                    .and_then(super::interaction::break_ticks)
                                    .is_some_and(|ticks| elapsed >= ticks)
                                && self.plugin_allows(|plugins| {
                                    plugins.block_broken(
                                        self.uuid,
                                        &self.name,
                                        ferrite_plugin::BlockPosition {
                                            x: pos.x,
                                            y: pos.y,
                                            z: pos.z,
                                        },
                                        material_name(state),
                                    )
                                })
                                && self.break_block(location, state)
                            {
                                self.broken.push((pos, state));
                            }
                        }
                        _ => {}
                    }
                    self.send_block_snapshot(p.location);
                    send(
                        self,
                        &AcknowledgeBlockChange {
                            sequence: p.sequence,
                        },
                    );
                }
                ServerboundPlay::UseItemOn(p) => {
                    let clicked = ferrite_protocol::types::BlockPos::unpack(p.location);
                    let state = self
                        .source
                        .as_ref()
                        .and_then(|s| s.block(clicked.x, clicked.y, clicked.z));
                    let allowed = self.plugin_allows(|plugins| {
                        plugins.player_interacted(
                            self.uuid,
                            &self.name,
                            InteractAction::RightClickBlock,
                            ferrite_plugin::BlockPosition {
                                x: clicked.x,
                                y: clicked.y,
                                z: clicked.z,
                            },
                            u8::try_from(p.face.0.clamp(0, 5)).unwrap_or(0),
                            material_name(state.unwrap_or(0)),
                        )
                    });
                    if allowed {
                        let used = (0..=1).contains(&p.hand.0)
                            && self.hit(clicked).is_some()
                            && self.use_block(p.location);
                        if !used {
                            self.place(&p);
                        }
                    } else {
                        self.send_block_snapshot(p.location);
                    }
                    self.send_block_snapshot(p.location);
                    if let Some(face) = usize::try_from(p.face.0)
                        .ok()
                        .and_then(|i| ferrite_math::Direction::ALL.get(i))
                    {
                        let pos = ferrite_protocol::types::BlockPos::unpack(p.location);
                        let (dx, dy, dz) = face.step();
                        self.send_block_snapshot(
                            ferrite_protocol::types::BlockPos::new(
                                pos.x + dx,
                                pos.y + dy,
                                pos.z + dz,
                            )
                            .pack(),
                        );
                    }
                    send(
                        self,
                        &AcknowledgeBlockChange {
                            sequence: p.sequence,
                        },
                    );
                }
                ServerboundPlay::SetHeldItem(p) => {
                    // Vanilla `handleSetCarriedItem` ignores slots outside the hotbar.
                    if (0..=8).contains(&p.slot) {
                        self.inventory.selected = p.slot as usize;
                    }
                }
                ServerboundPlay::CreativeSlot(p) => self.handle_creative_slot(p),
                ServerboundPlay::Confirm(p) => {
                    if self.pending_teleport == Some(p.id.0) {
                        self.pending_teleport = None;
                    }
                }
                ServerboundPlay::ChunkBatchReceived(rate) => {
                    if rate.is_finite() {
                        self.rate = rate.clamp(1.0, 64.0);
                        self.awaiting_batch = false;
                    }
                }
                ServerboundPlay::Position(p) => {
                    if self.pending_teleport.is_none() {
                        let to = [p.x, p.y, p.z];
                        if valid_move(start, to) {
                            // Bukkit fires `PlayerMoveEvent` only when the position changes.
                            let movement = ferrite_plugin::PlayerMove {
                                from: self.pos,
                                to,
                                from_yaw: self.yaw,
                                from_pitch: self.pitch,
                                to_yaw: self.yaw,
                                to_pitch: self.pitch,
                            };
                            if to != self.pos
                                && !self.plugin_allows(|plugins| {
                                    plugins.player_moved(self.uuid, &self.name, &movement)
                                })
                            {
                                self.teleport();
                                continue;
                            }
                            self.pos = to;
                            self.ground = p.flags & 1 != 0;
                            changed = true;
                        } else {
                            self.teleport();
                        }
                    }
                }
                ServerboundPlay::PositionRotation(p) => {
                    if self.pending_teleport.is_none() {
                        let to = [p.x, p.y, p.z];
                        if valid_move(start, to) && p.yaw.is_finite() && p.pitch.is_finite() {
                            let movement = ferrite_plugin::PlayerMove {
                                from: self.pos,
                                to,
                                from_yaw: self.yaw,
                                from_pitch: self.pitch,
                                to_yaw: p.yaw,
                                to_pitch: p.pitch,
                            };
                            if to != self.pos
                                && !self.plugin_allows(|plugins| {
                                    plugins.player_moved(self.uuid, &self.name, &movement)
                                })
                            {
                                self.teleport();
                                continue;
                            }
                            self.pos = to;
                            self.yaw = p.yaw;
                            self.pitch = p.pitch;
                            self.ground = p.flags & 1 != 0;
                            changed = true;
                        } else {
                            self.teleport();
                        }
                    }
                }
                ServerboundPlay::Rotation(p) => {
                    if p.yaw.is_finite() && p.pitch.is_finite() {
                        self.yaw = p.yaw;
                        self.pitch = p.pitch;
                        self.ground = p.flags & 1 != 0;
                        changed = true;
                    }
                }
                ServerboundPlay::Flags(p) => {
                    self.ground = p.flags & 1 != 0;
                }
                _ => {}
            }
        }
        if changed {
            self.positions.lock().unwrap().insert(self.uuid, self.pos);
            let _ = self.signals.send(Signal::Update {
                uuid: self.uuid,
                pos: self.pos,
                yaw: self.yaw,
                pitch: self.pitch,
                ground: self.ground,
            });
        }
        true
    }
}
/// Sends a tracker packet to a player: through its connection while it is in this region,
/// otherwise through the world-wide table (it left the region since it started tracking).
fn deliver(
    players: &HashMap<Uuid, Player>,
    ctx: &TickContext,
    (uuid, session): ViewerKey,
    bytes: &[u8],
) {
    let packet = Outbound::Packet(bytes.to_vec().into());
    match players.get(&uuid) {
        Some(p) if p.session == session => {
            let _ = p.outbound.send(packet.clone());
        }
        Some(_) => {}
        None => {
            if let Some((_, outbound)) = ctx.tracking.lock().unwrap().get(&uuid) {
                let _ = outbound.send(packet.clone());
            }
        }
    }
}
impl Region {
    pub fn hold_chunk(&mut self, chunk: ChunkPos) {
        *self.held_chunks.entry(chunk).or_default() += 1;
    }
    pub fn release_chunk(&mut self, chunk: ChunkPos) {
        if let Some(count) = self.held_chunks.get_mut(&chunk) {
            *count -= 1;
            if *count == 0 {
                self.held_chunks.remove(&chunk);
            }
        }
    }
    pub fn schedule_fluid(&mut self, pos: (i32, i32, i32)) {
        let chunk = ChunkPos(pos.0.div_euclid(16), pos.2.div_euclid(16));
        if self.held_chunks.contains_key(&chunk) {
            self.fluid_ticks.schedule(pos, 0);
        }
    }
    pub fn tick_random(
        &mut self,
        source: &super::anvil::AnvilChunkSource,
        tracking: &Tracking,
        speed: u32,
    ) {
        self.random_ticks
            .tick(source, &self.held_chunks, tracking, speed);
    }
    pub fn tick_fluids(&mut self, source: &super::anvil::AnvilChunkSource, tracking: &Tracking) {
        let mut world = super::fluid::RegionFluidWorld::new(source, &self.held_chunks, tracking);
        self.fluid_ticks.advance(&mut world);
    }
    /// Runs the neighbor updates of this tick's block changes (vanilla
    /// `Level.markAndNotifyBlock` -> `Block.updateNeighbourShapes`). Unsupported blocks break
    /// here; their loot drops as item entities through the normal loot tables.
    fn tick_block_updates(&mut self, ctx: &TickContext) -> Vec<[i32; 3]> {
        let mut changed = Vec::new();
        for p in self.players.values_mut() {
            changed.append(&mut p.changed_blocks);
        }
        let Some(source) = ctx.source.as_deref() else {
            return changed;
        };
        for pos in &changed {
            self.updates.block_changed(*pos);
        }
        let mut removed = Vec::new();
        self.apply_neighbor_updates(source, ctx, &mut removed);
        for (pos, state) in removed {
            self.spawn_block_drops(pos, state);
            changed.push(pos);
        }
        changed
    }

    /// Runs the queued shape updates against the region's blocks.
    fn apply_neighbor_updates(
        &mut self,
        source: &super::anvil::AnvilChunkSource,
        ctx: &TickContext,
        removed: &mut Vec<([i32; 3], u16)>,
    ) {
        let mut world =
            super::fluid::RegionFluidWorld::new(source, &self.held_chunks, &ctx.tracking);
        self.updates.apply(&mut world, removed);
    }

    /// Drops the loot table of a block broken by a neighbor or gravity update.
    fn spawn_block_drops(&mut self, pos: [i32; 3], state: u16) {
        for stack in super::drops::block_drops(state, ItemStack::default(), &mut self.rng) {
            let record = item::pop_resource(pos, stack, &mut self.rng);
            self.entities.insert(record);
        }
    }

    /// Vanilla `FallingBlock.onPlace`/`neighborChanged`: schedules a free gravity block. The
    /// changed block and its six neighbors are the blocks vanilla notifies.
    fn schedule_gravity(&mut self, source: &super::anvil::AnvilChunkSource, changed: &[[i32; 3]]) {
        for &pos in changed {
            for (dx, dy, dz) in [
                (0, 0, 0),
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ] {
                let target = [pos[0] + dx, pos[1] + dy, pos[2] + dz];
                let chunk = ChunkPos(target[0].div_euclid(16), target[2].div_euclid(16));
                if !self.held_chunks.contains_key(&chunk) {
                    continue;
                }
                let Some(state) = source
                    .block(target[0], target[1], target[2])
                    .and_then(|s| u16::try_from(s).ok())
                else {
                    continue;
                };
                if !super::gravity::is_gravity(state) {
                    continue;
                }
                let below = source
                    .block(target[0], target[1] - 1, target[2])
                    .and_then(|s| u16::try_from(s).ok());
                if super::gravity::is_free(below) {
                    self.gravity.schedule(target);
                }
            }
        }
    }

    /// Runs due gravity checks, then the neighbor updates of the blocks they removed.
    fn tick_gravity(&mut self, ctx: &TickContext) {
        let Some(source) = ctx.source.as_deref() else {
            return;
        };
        let removed =
            self.gravity
                .tick(source, &self.held_chunks, &ctx.tracking, &mut self.entities);
        if removed.is_empty() {
            return;
        }
        for pos in &removed {
            self.updates.block_changed(*pos);
        }
        let mut broken = Vec::new();
        self.apply_neighbor_updates(source, ctx, &mut broken);
        for (pos, state) in broken {
            self.spawn_block_drops(pos, state);
        }
    }

    /// Ticks the falling block entities against this region's blocks.
    fn tick_falling(&mut self, ctx: &TickContext) {
        let Some(source) = ctx.source.as_deref() else {
            return;
        };
        let blocks = TickBlocks {
            source: Some(source),
            seen: Default::default(),
        };
        let world = super::fluid::RegionFluidWorld::new(source, &self.held_chunks, &ctx.tracking);
        let mut adapter = FallingAdapter {
            read: &blocks,
            world,
        };
        ferrite_entity::falling::tick_falling_blocks(&mut self.entities, &mut adapter, -64, 320);
    }
    /// Spawns the loot of the blocks a player broke in survival as item entities.
    fn drop_broken_blocks(&mut self, p: &mut Player) {
        let held = p.inventory.selected_item();
        let tool = ItemStack {
            count: held.count.max(0),
            item_id: i32::from(held.item),
        };
        for (pos, state) in p.broken.drain(..) {
            let Ok(state) = u16::try_from(state) else {
                continue;
            };
            for stack in super::drops::block_drops(state, tool, &mut self.rng) {
                let record = item::pop_resource([pos.x, pos.y, pos.z], stack, &mut self.rng);
                self.entities.insert(record);
            }
        }
    }
    /// Lets every non-spectator player pick up the items it touches.
    fn pick_up_items(&mut self, ctx: &TickContext) {
        let mut sent = Vec::new();
        for p in self.players.values_mut() {
            if p.gamemode == 3 {
                continue;
            }
            let mut picker = item::Picker {
                uuid: p.uuid,
                entity_id: p.entity,
                pos: ferrite_math::Vec3::new(p.pos[0], p.pos[1], p.pos[2]),
                inventory: &mut p.inventory,
                infinite: p.gamemode == 1,
            };
            item::pick_up(&mut self.entities, &mut picker, &mut |viewer, bytes| {
                sent.push((viewer, bytes.to_vec()))
            });
        }
        // Like vanilla: the take animation first, the inventory sync at the end of the tick.
        for (viewer, bytes) in sent {
            deliver(&self.players, ctx, viewer, &bytes);
        }
    }
    /// Spawns the TNT a player lit (vanilla `PrimedTnt` at the block position).
    fn spawn_primed_tnt(&mut self, p: &mut Player) {
        for (pos, fuse) in p.primed_tnt.drain(..) {
            let center = ferrite_math::Vec3::new(
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]),
                f64::from(pos[2]) + 0.5,
            );
            self.entities
                .insert(ferrite_entity::tnt::new_primed_tnt(center, fuse));
        }
    }
    /// Ticks primed TNT and runs its `Explosion` where a fuse ends.
    fn tick_tnt(&mut self, ctx: &TickContext) {
        let Some(source) = ctx.source.as_deref() else {
            return;
        };
        let blocks = TickBlocks {
            source: Some(source),
            seen: Default::default(),
        };
        let centers = ferrite_entity::tnt::tick_tnt(&mut self.entities, &blocks);
        if centers.is_empty() {
            return;
        }
        for center in centers {
            self.explode(
                [center.x, center.y, center.z],
                super::explosion::TNT_POWER,
                ctx,
            );
        }
        // Vanilla `Explosion.finalizeExplosion` sets blocks with `Block.UPDATE_NEIGHBORS`.
        let mut broken = Vec::new();
        self.apply_neighbor_updates(source, ctx, &mut broken);
        for (pos, state) in broken {
            self.spawn_block_drops(pos, state);
        }
    }
    /// Vanilla `Explosion`: entity knockback and exposure first, then the ray cast blocks and
    /// their loot-table drops.
    fn explode(&mut self, center: [f64; 3], power: f32, ctx: &TickContext) {
        let Some(source) = ctx.source.as_deref() else {
            return;
        };
        // Players are not in the entity store; vanilla pushes them like any other entity.
        let hits: Vec<(i32, [f64; 3])> = self
            .players
            .values()
            .filter_map(|p| {
                let bbox = ferrite_math::AABB::new(
                    p.pos[0] - 0.3,
                    p.pos[1],
                    p.pos[2] - 0.3,
                    p.pos[0] + 0.3,
                    p.pos[1] + 1.8,
                    p.pos[2] + 0.3,
                );
                let impulse = super::explosion::impulse(
                    source,
                    center,
                    power,
                    ferrite_math::Vec3::new(p.pos[0], p.pos[1], p.pos[2]),
                    bbox,
                )?;
                Some((p.entity, impulse))
            })
            .collect();
        for (entity, impulse) in hits {
            if let Some(p) = self.players.values().find(|p| p.entity == entity) {
                send(
                    p,
                    &SetEntityMotion {
                        id: VarInt(entity),
                        velocity: ferrite_protocol::types::LpVec3(impulse),
                    },
                );
            }
        }
        // Entity push (vanilla `Entity.push`).
        let radius = f64::from(power) * 2.0;
        let area = ferrite_math::AABB::new(
            center[0] - radius,
            center[1] - radius,
            center[2] - radius,
            center[0] + radius,
            center[1] + radius,
            center[2] + radius,
        );
        for key in self.entities.query_vec(area) {
            if self
                .entities
                .flags(key)
                .is_some_and(|flags| flags.has(ferrite_entity::Flags::REMOVED))
            {
                continue;
            }
            let (Some(pos), Some(bbox)) = (self.entities.pos(key), self.entities.bbox(key)) else {
                continue;
            };
            let Some(impulse) = super::explosion::impulse(source, center, power, pos, bbox) else {
                continue;
            };
            let vel = self.entities.vel(key).unwrap_or_default();
            self.entities.set_vel(
                key,
                ferrite_math::Vec3::new(vel.x + impulse[0], vel.y + impulse[1], vel.z + impulse[2]),
            );
            if let Some(flags) = self.entities.flags_mut(key) {
                flags.set(ferrite_entity::Flags::NEEDS_SYNC, true);
            }
        }
        // Destroyed blocks, with the state read before it turns into air.
        let air = u32::from(ferrite_data::blocks::default_state("air").expect("vanilla air"));
        let destroyed;
        let mut removed = Vec::new();
        {
            let mut world =
                super::fluid::RegionFluidWorld::new(source, &self.held_chunks, &ctx.tracking);
            destroyed = super::explosion::explode_blocks(&world, center, power, &mut self.rng);
            for pos in &destroyed {
                // Never touch another region's blocks.
                let chunk = ChunkPos(pos[0].div_euclid(16), pos[2].div_euclid(16));
                if !self.held_chunks.contains_key(&chunk) {
                    continue;
                }
                let Some(state) =
                    super::neighbors::BlockWorld::block(&world, pos[0], pos[1], pos[2])
                else {
                    continue;
                };
                ferrite_fluid::FluidWorld::set_block(&mut world, pos[0], pos[1], pos[2], air);
                removed.push((*pos, state));
            }
        }
        for (pos, state) in removed {
            self.spawn_block_drops(pos, state);
        }
        for pos in &destroyed {
            self.updates.block_changed(*pos);
        }
        self.schedule_gravity(source, &destroyed);
    }
    /// Spawns what a player threw (Q, clicks outside a screen, overflowing crafts) as item
    /// entities in front of it. Item entities hold plain stacks for now, so components are not
    /// carried over.
    fn spawn_thrown(&mut self, p: &mut Player) {
        for (block, stack) in p.menus.spilled.drain(..) {
            let plain = ItemStack {
                count: stack.count,
                item_id: i32::from(stack.item),
            };
            for record in item::drop_from_container(block, plain, &mut self.rng) {
                self.entities.insert(record);
            }
        }
        for stack in p.menus.thrown.drain(..) {
            let plain = ItemStack {
                count: stack.count,
                item_id: i32::from(stack.item),
            };
            let feet = ferrite_math::Vec3::new(p.pos[0], p.pos[1], p.pos[2]);
            let record = item::thrown_by_player(feet, 1.62, p.yaw, p.pitch, plain, &mut self.rng);
            self.entities.insert(record);
        }
    }
    /// Runs the entity tracker against this region's players.
    fn track_entities(&mut self, ctx: &TickContext) {
        let viewers: Vec<_> = self
            .players
            .values()
            .map(|p| Viewer {
                key: (p.uuid, p.session),
                pos: ferrite_math::Vec3::new(p.pos[0], p.pos[1], p.pos[2]),
                view_distance: p.view,
                chunks: &p.loaded,
            })
            .collect();
        let players = &self.players;
        tracker::track(&mut self.entities, &viewers, &mut |viewer, bytes| {
            deliver(players, ctx, viewer, bytes)
        });
    }
    pub fn tick(&mut self, ctx: &TickContext) {
        let timings = self.timings.clone();
        let _tick = timings.as_ref().map(|t| t.scope("region-tick"));
        let ids: Vec<_> = self.players.keys().copied().collect();
        for id in ids {
            let mut p = self.players.remove(&id).unwrap();
            let updated = match &timings {
                Some(timings) => {
                    let _scope = timings.scope("player-update");
                    p.update()
                }
                None => p.update(),
            };
            self.spawn_thrown(&mut p);
            self.spawn_primed_tnt(&mut p);
            if !updated {
                let held = p.loaded.iter().map(|(x, z)| ChunkPos(*x, *z)).collect();
                let _ = p.signals.send(Signal::Leave {
                    uuid: p.uuid,
                    chunk: p.chunk,
                    held,
                });
                continue;
            }
            let next = ChunkPos(
                (p.pos[0].floor() as i32).div_euclid(16),
                (p.pos[2].floor() as i32).div_euclid(16),
            );
            if next != p.chunk {
                let from = p.chunk;
                p.chunk = next;
                p.refresh_chunks();
                let tx = p.signals.clone();
                let _ = tx.send(Signal::Transfer {
                    from,
                    to: next,
                    player: Box::new(p),
                });
                continue;
            }
            match &timings {
                Some(timings) => {
                    let _scope = timings.scope("chunk-stream");
                    p.chunks_tick(&mut self.generator);
                }
                None => p.chunks_tick(&mut self.generator),
            }
            self.drop_broken_blocks(&mut p);
            self.players.insert(id, p);
        }
        let changed = self.tick_block_updates(ctx);
        if let Some(source) = ctx.source.as_deref() {
            self.schedule_gravity(source, &changed);
        }
        self.tick_gravity(ctx);
        let blocks = TickBlocks {
            source: ctx.source.as_deref(),
            seen: Default::default(),
        };
        item::tick_items(&mut self.entities, &blocks, &mut self.rng);
        self.pick_up_items(ctx);
        self.tick_falling(ctx);
        self.tick_tnt(ctx);
        for p in self.players.values_mut() {
            p.tick_containers();
        }
        self.track_entities(ctx);
        self.entities.sweep_removed();
        for entity in self.entities.take_crossing() {
            let (x, z) = entity.chunk();
            let _ = ctx.signals.send(Signal::EntityTransfer {
                to: ChunkPos(x, z),
                entity: Box::new(entity),
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn experience_curve_matches_vanilla_levels() {
        assert_eq!(total_for_level(0), 0);
        assert_eq!(total_for_level(1), 7);
        assert_eq!(total_for_level(16), 352);
        assert_eq!(total_for_level(17), 394);
        assert_eq!(total_for_level(30), 1395);
        assert_eq!(total_for_level(31), 1507);
        assert_eq!(total_for_level(32), 1628);
        assert_eq!(level_and_progress(394), (17, 0.0));
        assert_eq!(level_and_progress(7), (1, 0.0));
        assert!((level_and_progress(15).1 - 8.0 / 9.0).abs() < 1e-6);
    }

    #[test]
    fn tick_records_a_region_scope_when_timings_are_attached() {
        let mut region = Region::default();
        let timings = Arc::new(Timings::new());
        region.timings = Some(timings.clone());

        let (signals, _rx) = tokio::sync::mpsc::unbounded_channel();
        region.tick(&TickContext {
            signals,
            tracking: Default::default(),
            source: None,
        });

        let report = timings.report_all(50.0);
        assert!(
            report.rows.iter().any(|row| row.path == "region-tick"),
            "{report}"
        );
    }

    #[test]
    fn window_is_sorted_and_unloads_outside() {
        let old = std::collections::HashSet::from([(0, 0), (1, 0), (-1, 0)]);
        let (forget, new) = chunk_window((3, 0), 2, &old);
        assert!(forget.contains(&(-1, 0)));
        assert_eq!(new.first(), Some(&(3, 0)));
        assert!(!new.contains(&(1, 0)));
    }
}

#[cfg(test)]
mod fluid_ownership_tests {
    use super::*;
    use ferrite_fluid::{FluidKind, FluidWorld, fluid_state};
    use std::collections::HashMap;

    struct Grid(HashMap<(i32, i32, i32), u32>);
    impl FluidWorld for Grid {
        fn block(&self, x: i32, y: i32, z: i32) -> u32 {
            *self.0.get(&(x, y, z)).unwrap_or(&0)
        }
        fn set_block(&mut self, x: i32, y: i32, z: i32, state: u32) {
            self.0.insert((x, y, z), state);
        }
        fn ultrawarm(&self) -> bool {
            false
        }
    }
    #[test]
    fn fluid_ticks_survive_merge_and_split() {
        let mut left = Region::default();
        let mut right = Region::default();
        left.fluid_ticks.schedule((0, 0, 0), 7);
        right.fluid_ticks.schedule((512, 0, 0), 9);
        left.merge(right);
        let mut right = left.split(&HashSet::from([SectionPos(2, 0)]));
        let water = u32::from(ferrite_data::blocks::default_state("water").unwrap());
        let stone = u32::from(ferrite_data::blocks::default_state("stone").unwrap());
        let mut grid = Grid(HashMap::from([
            ((0, 0, 0), water),
            ((512, 0, 0), water),
            ((0, -1, 0), stone),
            ((512, -1, 0), stone),
        ]));
        left.fluid_ticks.run_until(&mut grid, 6);
        right.fluid_ticks.run_until(&mut grid, 8);
        assert_eq!(fluid_state(grid.block(0, 0, 1)).kind, FluidKind::Empty);
        assert_eq!(fluid_state(grid.block(512, 0, 1)).kind, FluidKind::Empty);
        left.fluid_ticks.run_until(&mut grid, 7);
        right.fluid_ticks.run_until(&mut grid, 9);
        assert_eq!(fluid_state(grid.block(0, 0, 1)).kind, FluidKind::Water);
        assert_eq!(fluid_state(grid.block(512, 0, 1)).kind, FluidKind::Water);
    }
}

#[test]
fn chunk_holders_follow_region_merge_and_split() {
    let mut left = Region::default();
    let mut right = Region::default();
    left.hold_chunk(ChunkPos(0, 0));
    left.hold_chunk(ChunkPos(0, 0));
    right.hold_chunk(ChunkPos(32, 0));
    left.merge(right);
    let mut right = left.split(&HashSet::from([SectionPos(2, 0)]));
    assert_eq!(left.held_chunks.get(&ChunkPos(0, 0)), Some(&2));
    assert!(!left.held_chunks.contains_key(&ChunkPos(32, 0)));
    assert_eq!(right.held_chunks.get(&ChunkPos(32, 0)), Some(&1));
    right.release_chunk(ChunkPos(32, 0));
    assert!(right.held_chunks.is_empty());
}

#[cfg(test)]
mod event_policy_tests {
    use super::*;
    use ferrite_plugin::{BlockPosition, CommandSender, PluginCommand, PluginCommandResult};

    /// A bridge whose every call fails, standing in for a broken JVM call.
    struct Failing;
    impl ferrite_plugin::ServerHooks for Failing {
        fn player_joined(&self, _: Uuid, _: &str) -> Result<(), ferrite_plugin::Error> {
            Err(ferrite_plugin::Error::InvalidTask)
        }
        fn player_left(&self, _: Uuid, _: &str) -> Result<(), ferrite_plugin::Error> {
            Err(ferrite_plugin::Error::InvalidTask)
        }
        fn chat(&self, _: Uuid, _: &str, _: &str) -> Result<Option<String>, ferrite_plugin::Error> {
            Err(ferrite_plugin::Error::InvalidTask)
        }
        fn player_moved(
            &self,
            _: Uuid,
            _: &str,
            _: &ferrite_plugin::PlayerMove,
        ) -> Result<bool, ferrite_plugin::Error> {
            Err(ferrite_plugin::Error::InvalidTask)
        }
        fn player_interacted(
            &self,
            _: Uuid,
            _: &str,
            _: InteractAction,
            _: BlockPosition,
            _: u8,
            _: &str,
        ) -> Result<bool, ferrite_plugin::Error> {
            Err(ferrite_plugin::Error::InvalidTask)
        }
        fn block_broken(
            &self,
            _: Uuid,
            _: &str,
            _: BlockPosition,
            _: &str,
        ) -> Result<bool, ferrite_plugin::Error> {
            Err(ferrite_plugin::Error::InvalidTask)
        }
        fn block_placed(
            &self,
            _: Uuid,
            _: &str,
            _: BlockPosition,
            _: u8,
            _: &str,
            _: &str,
        ) -> Result<bool, ferrite_plugin::Error> {
            Err(ferrite_plugin::Error::InvalidTask)
        }
        fn player_command(&self, _: Uuid, _: &str, _: &str) -> Result<bool, ferrite_plugin::Error> {
            Err(ferrite_plugin::Error::InvalidTask)
        }
        fn plugin_commands(&self) -> Result<Vec<PluginCommand>, ferrite_plugin::Error> {
            Err(ferrite_plugin::Error::InvalidTask)
        }
        fn run_plugin_command(
            &self,
            _: CommandSender,
            _: &str,
        ) -> Result<PluginCommandResult, ferrite_plugin::Error> {
            Err(ferrite_plugin::Error::InvalidTask)
        }
    }

    fn decide(policy: PluginEventPolicy, result: Result<bool, ferrite_plugin::Error>) -> bool {
        let failing: &dyn ferrite_plugin::ServerHooks = &Failing;
        event_allows(Some(failing), policy, |_| result)
    }

    #[test]
    fn failed_plugin_events_follow_the_configured_policy() {
        assert!(decide(
            PluginEventPolicy::Allow,
            Err(ferrite_plugin::Error::InvalidTask)
        ));
        assert!(!decide(
            PluginEventPolicy::Deny,
            Err(ferrite_plugin::Error::InvalidTask)
        ));
        assert!(!decide(PluginEventPolicy::Allow, Ok(false)));
        assert!(decide(PluginEventPolicy::Deny, Ok(true)));
    }

    #[test]
    fn a_missing_plugin_bridge_always_allows_the_action() {
        assert!(event_allows(None, PluginEventPolicy::Deny, |_| Err(
            ferrite_plugin::Error::InvalidTask
        )));
    }

    #[test]
    fn block_materials_come_from_the_state_table() {
        let stone = ferrite_data::blocks::default_state("stone").unwrap();
        let grass = ferrite_data::blocks::default_state("grass_block").unwrap();
        assert_eq!(material_name(u32::from(stone)), "minecraft:stone");
        assert_eq!(material_name(u32::from(grass)), "minecraft:grass_block");
        assert_eq!(material_name(u32::MAX), "minecraft:air");
    }
}
