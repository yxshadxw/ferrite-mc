use super::{
    chunk::{ChunkSource, FlatGenerator},
    valid_move,
};
use ferrite_entity::{
    BlockGetter, EntityRecord, EntityStore, Rng, item,
    tracker::{self, Viewer, ViewerKey},
};
use ferrite_net::game::{Outbound, ServerboundPlay, packet};
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
    Teleport([f64; 3]),
}
pub type Mailbox = Arc<Mutex<VecDeque<Command>>>;
pub type Tracking = Arc<Mutex<HashMap<Uuid, (HashSet<(i32, i32)>, UnboundedSender<Outbound>)>>>;
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
    pub name: String,
    /// Plugin event hooks; `None` when the server runs without plugins.
    pub plugins: Option<Arc<dyn ferrite_plugin::ServerHooks>>,
    pub positions: Arc<Mutex<HashMap<Uuid, [f64; 3]>>>,
    pub tracking: Tracking,
    pub source: Option<Arc<super::anvil::AnvilChunkSource>>,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub ground: bool,
    pub chunk: ChunkPos,
    pub mailbox: Mailbox,
    pub outbound: UnboundedSender<Outbound>,
    pub signals: UnboundedSender<Signal>,
    pub biome: i32,
    pub gamemode: i32,
    pub inventory: [ItemStack; 46],
    pub held_slot: u8,
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
fn send<P: ferrite_protocol::Packet>(p: &Player, msg: &P) {
    let _ = p.outbound.send(packet(msg));
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
            let _ = self.outbound.send(Outbound::Packet(packet));
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
    fn change_block(&self, location: i64, expected: u32, state: u32) -> bool {
        let pos = ferrite_protocol::types::BlockPos::unpack(location);
        let tracking = self.tracking.lock().unwrap();
        if self.source.as_ref().is_some_and(|source| {
            source
                .compare_set_block(pos.x, pos.y, pos.z, Some(expected), state)
                .is_ok()
        }) {
            let chunk = (pos.x.div_euclid(16), pos.z.div_euclid(16));
            for (loaded, outbound) in tracking.values() {
                if loaded.contains(&chunk) {
                    let _ = outbound.send(packet(&BlockUpdate {
                        location,
                        block_state: VarInt(state as i32),
                    }));
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
        let slot = if p.hand.0 == 0 {
            36 + self.held_slot as usize
        } else {
            45
        };
        let item = self.inventory[slot];
        if item.count <= 0 {
            return;
        }
        let water_bucket = ferrite_data::registries::id("minecraft:item", "minecraft:water_bucket")
            == Some(item.item_id);
        let state = if water_bucket {
            ferrite_data::blocks::default_state("water").expect("vanilla water")
        } else {
            let Some(state) = super::interaction::placement_state(
                item.item_id,
                p.face.0,
                p.cursor_y,
                self.yaw,
                old,
            ) else {
                return;
            };
            state
        };
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
        let placed = self.change_block(target.pack(), u32::from(old), u32::from(state));
        drop(positions);
        if placed && self.gamemode == 0 {
            if water_bucket {
                self.inventory[slot] = ItemStack {
                    count: 1,
                    item_id: ferrite_data::registries::id("minecraft:item", "minecraft:bucket")
                        .expect("vanilla bucket"),
                };
            } else {
                self.inventory[slot].count -= 1;
                if self.inventory[slot].count == 0 {
                    self.inventory[slot] = ItemStack::default();
                }
            }
            send(
                self,
                &ContainerSlot {
                    window: VarInt(0),
                    state: VarInt(0),
                    slot: slot as i16,
                    item: self.inventory[slot],
                },
            );
        }
    }
    /// Runs an event hook; a missing bridge or a failed call allows the action.
    fn plugin_allows(
        &self,
        event: impl FnOnce(&dyn ferrite_plugin::ServerHooks) -> Result<bool, ferrite_plugin::Error>,
    ) -> bool {
        let Some(plugins) = &self.plugins else {
            return true;
        };
        match event(plugins.as_ref()) {
            Ok(allowed) => allowed,
            Err(error) => {
                tracing::warn!(%error, "plugin event failed");
                true
            }
        }
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
                Command::Teleport(pos) => {
                    if pos.iter().all(|v| v.is_finite() && v.abs() <= 30_000_000.0) {
                        self.pos = pos;
                        self.teleport();
                        changed = true;
                    }
                    continue;
                }
                Command::Input(msg) => msg,
            };
            match msg {
                ServerboundPlay::PlayerAction(p) => {
                    let pos = ferrite_protocol::types::BlockPos::unpack(p.location);
                    match p.status.0 {
                        0 if self.hit(pos).is_some() => {
                            let state = self
                                .source
                                .as_ref()
                                .and_then(|s| s.block(pos.x, pos.y, pos.z));
                            if let Some(state) = state {
                                if self.gamemode == 1 {
                                    self.change_block(p.location, state, 0);
                                } else if self.gamemode == 0 {
                                    // Zero hardness breaks on the first hit (vanilla: destroy
                                    // progress >= 1 on START), so no finish action follows.
                                    let instant = u16::try_from(state).is_ok_and(|s| {
                                        usize::from(s) < ferrite_data::blocks::STATE_COUNT
                                            && ferrite_data::block_props::get(s).hardness == 0.0
                                    });
                                    if instant {
                                        if self.change_block(p.location, state, 0) {
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
                                && self.change_block(location, state, 0)
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
                    self.place(&p);
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
                    if (0..=8).contains(&p.slot) {
                        self.held_slot = p.slot as u8;
                    }
                }
                ServerboundPlay::CreativeSlot(p) => {
                    if self.gamemode == 1 && (0..46).contains(&p.slot) {
                        self.inventory[p.slot as usize] = p.item;
                        send(
                            self,
                            &ContainerSlot {
                                window: VarInt(0),
                                state: VarInt(0),
                                slot: p.slot,
                                item: p.item,
                            },
                        );
                    }
                }
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
    let packet = Outbound::Packet(bytes.to_vec());
    match players.get(&uuid) {
        Some(p) if p.session == session => {
            let _ = p.outbound.send(packet);
        }
        Some(_) => {}
        None => {
            if let Some((_, outbound)) = ctx.tracking.lock().unwrap().get(&uuid) {
                let _ = outbound.send(packet);
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
    /// Spawns the loot of the blocks a player broke in survival as item entities.
    fn drop_broken_blocks(&mut self, p: &mut Player) {
        let tool = p.inventory[36 + usize::from(p.held_slot.min(8))];
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
        let mut slots = Vec::new();
        for p in self.players.values_mut() {
            if p.gamemode == 3 {
                continue;
            }
            let mut picker = item::Picker {
                uuid: p.uuid,
                entity_id: p.entity,
                pos: ferrite_math::Vec3::new(p.pos[0], p.pos[1], p.pos[2]),
                inventory: &mut p.inventory,
                selected: usize::from(p.held_slot),
                infinite: p.gamemode == 1,
            };
            let changed = item::pick_up(&mut self.entities, &mut picker, &mut |viewer, bytes| {
                sent.push((viewer, bytes.to_vec()))
            });
            if !changed.is_empty() {
                slots.push((p.uuid, changed));
            }
        }
        // Like vanilla: the take animation first, the inventory sync at the end of the tick.
        for (viewer, bytes) in sent {
            deliver(&self.players, ctx, viewer, &bytes);
        }
        for (uuid, changed) in slots {
            let p = &self.players[&uuid];
            for slot in changed {
                send(
                    p,
                    &ContainerSlot {
                        window: VarInt(0),
                        state: VarInt(0),
                        slot: slot as i16,
                        item: p.inventory[slot],
                    },
                );
            }
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
        let blocks = TickBlocks {
            source: ctx.source.as_deref(),
            seen: Default::default(),
        };
        item::tick_items(&mut self.entities, &blocks, &mut self.rng);
        self.pick_up_items(ctx);
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
