mod anvil;
pub use anvil::{AnvilChunkSource, read_spawn as load_spawn};
pub mod playerdata;
pub fn new_anvil_source(
    path: std::path::PathBuf,
    registry: ferrite_world::RuntimeRegistry,
) -> Arc<AnvilChunkSource> {
    AnvilChunkSource::new(path, registry)
}
mod chunk;
mod commands;
pub mod interaction;
mod profile;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn movement_validation_rejects_large_and_nonfinite_moves() {
        assert!(valid_move([0.0, 65.0, 0.0], [5.0, 65.0, 0.0]));
        assert!(!valid_move([0.0, 65.0, 0.0], [101.0, 65.0, 0.0]));
        assert!(!valid_move([0.0, 65.0, 0.0], [f64::NAN, 65.0, 0.0]));
    }
}
fn valid_move(from: [f64; 3], to: [f64; 3]) -> bool {
    to.iter().all(|v| v.is_finite() && v.abs() <= 30_000_000.0)
        && from
            .iter()
            .zip(to)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<f64>()
            <= 10_000.0
}
mod fluid;
mod global;
mod random_tick;
mod region;

use self::{
    global::{GlobalPlayer, GlobalRegion},
    region::{Command, Mailbox, Player, Region, Signal},
};
use ferrite_net::game::{self, GameMessage, Outbound, ServerboundPlay};
use ferrite_protocol::{Encode, VarInt, packets::play::*};
use ferrite_region::{ChunkPos, Regionizer, Scheduler};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;
#[derive(Debug, Clone)]
pub struct Operator {
    pub uuid: Uuid,
    pub level: u8,
}
#[derive(Clone)]
pub struct WorldSettings {
    pub gamemode: i32,
    pub ops: Vec<Operator>,
    /// Plugin event and command hooks; `None` when the server runs without plugins.
    pub plugins: Option<Arc<dyn ferrite_plugin::ServerHooks>>,
}
impl WorldSettings {
    fn permission(&self, uuid: Uuid) -> u8 {
        self.ops
            .iter()
            .find(|op| op.uuid == uuid)
            .map_or(0, |op| op.level)
    }
}
impl Default for WorldSettings {
    fn default() -> Self {
        Self {
            gamemode: 1,
            ops: Vec::new(),
            plugins: None,
        }
    }
}
pub struct World {
    pub sender: mpsc::UnboundedSender<GameMessage>,
    task: tokio::task::JoinHandle<()>,
    pub tick_samples: Arc<Mutex<Vec<f64>>>,
    profiler: Arc<profile::Profiler>,
    random_tick_speed: Arc<AtomicU32>,
}
impl World {
    pub fn start() -> Self {
        Self::start_with(None, [0, 65, 0])
    }

    /// Tick statistics and profiling sessions recorded by this world.
    pub fn profiler(&self) -> Arc<profile::Profiler> {
        Arc::clone(&self.profiler)
    }
    pub fn start_with(source: Option<Arc<anvil::AnvilChunkSource>>, spawn: [i32; 3]) -> Self {
        Self::start_configured(source, spawn, WorldSettings::default())
    }
    pub fn start_configured(
        source: Option<Arc<anvil::AnvilChunkSource>>,
        spawn: [i32; 3],
        settings: WorldSettings,
    ) -> Self {
        let flat = source.is_none();
        let source = Some(source.unwrap_or_else(anvil::AnvilChunkSource::new_memory));
        let (sender, rx) = mpsc::unbounded_channel();
        let tick_samples = Arc::new(Mutex::new(Vec::new()));
        let profiler = Arc::new(profile::Profiler::new());
        let random_tick_speed = Arc::new(AtomicU32::new(3));
        let metrics = LoopMetrics {
            random_tick_speed: Arc::clone(&random_tick_speed),
            tick_samples: tick_samples.clone(),
            profiler: Arc::clone(&profiler),
        };
        let task = tokio::spawn(run(
            rx,
            sender.clone(),
            source,
            flat,
            spawn,
            metrics,
            settings,
        ));
        Self {
            sender,
            task,
            tick_samples,
            profiler,
            random_tick_speed,
        }
    }
    pub fn set_random_tick_speed(&self, speed: u32) {
        self.random_tick_speed.store(speed, Ordering::Relaxed);
    }
    pub async fn console_command(&self, command: String) -> anyhow::Result<bool> {
        let (complete, done) = oneshot::channel();
        self.sender
            .send(GameMessage::Console { command, complete })
            .map_err(|_| anyhow::anyhow!("game loop stopped"))?;
        done.await
            .map_err(|_| anyhow::anyhow!("console response lost"))?
            .map_err(anyhow::Error::msg)
    }
    pub async fn shutdown(self) -> anyhow::Result<()> {
        let (complete, done) = oneshot::channel();
        self.sender
            .send(GameMessage::Shutdown { complete })
            .map_err(|_| anyhow::anyhow!("game loop stopped"))?;
        let result = done
            .await
            .map_err(|_| anyhow::anyhow!("shutdown response lost"))?;
        self.task.await?;
        result.map_err(anyhow::Error::msg)
    }
}
fn emit<P: ferrite_protocol::Packet>(tx: &mpsc::UnboundedSender<Outbound>, p: &P) {
    let _ = tx.send(game::packet(p));
}
fn plugin_command_line(name: &str, args: &[String]) -> String {
    if args.is_empty() {
        name.to_owned()
    } else {
        format!("{name} {}", args.join(" "))
    }
}
fn plugin_message_text(content: &ferrite_plugin::MessageContent) -> String {
    match content {
        ferrite_plugin::MessageContent::Plain(text) => text.clone(),
        ferrite_plugin::MessageContent::AdventureJson(json) => json_text(json),
    }
}
fn json_text(json: &str) -> String {
    fn collect(value: &serde_json::Value, out: &mut String) {
        match value {
            serde_json::Value::String(text) => out.push_str(text),
            serde_json::Value::Array(values) => {
                for value in values {
                    collect(value, out);
                }
            }
            serde_json::Value::Object(map) => {
                if let Some(value) = map.get("text") {
                    collect(value, out);
                }
                if let Some(value) = map.get("extra") {
                    collect(value, out);
                }
            }
            _ => {}
        }
    }
    let mut out = String::new();
    if let Ok(value) = serde_json::from_str(json) {
        collect(&value, &mut out);
    }
    if out.is_empty() { json.to_owned() } else { out }
}
fn chat(tx: &mpsc::UnboundedSender<Outbound>, text: &str) {
    emit(
        tx,
        &SystemChat {
            content: ferrite_protocol::types::PlainText(text.into()),
            overlay: false,
        },
    );
}
/// Per-tick measurements owned by the game loop's instrumentation.
struct LoopMetrics {
    random_tick_speed: Arc<AtomicU32>,
    /// Raw region tick durations consumed by the load-test example.
    tick_samples: Arc<Mutex<Vec<f64>>>,
    profiler: Arc<profile::Profiler>,
}
async fn run(
    mut events: mpsc::UnboundedReceiver<GameMessage>,
    event_tx: mpsc::UnboundedSender<GameMessage>,
    source: Option<Arc<anvil::AnvilChunkSource>>,
    flat: bool,
    spawn: [i32; 3],
    metrics: LoopMetrics,
    settings: WorldSettings,
) {
    let LoopMetrics {
        random_tick_speed,
        tick_samples,
        profiler,
    } = metrics;
    let region_profiler = Arc::clone(&profiler);
    let tracking: region::Tracking = Arc::new(Mutex::new(HashMap::new()));
    let fluid_tracking = tracking.clone();
    let fluid_source = source.clone();
    let scheduler = Scheduler::with_default_threads(
        Regionizer::<Region>::default(),
        GlobalRegion::default(),
        move |region| {
            let now = std::time::Instant::now();
            let slot = region_profiler.region_slot(&mut region.profile_slot);
            region
                .timings
                .get_or_insert_with(|| Arc::clone(region_profiler.timings()));
            Region::tick(region);
            if let Some(source) = &fluid_source {
                region.tick_fluids(source, &fluid_tracking);
                region.tick_random(
                    source,
                    &fluid_tracking,
                    random_tick_speed.load(Ordering::Relaxed),
                );
            }
            let elapsed = now.elapsed();
            region_profiler.record_region(slot, elapsed);
            let mut samples = tick_samples.lock().unwrap();
            if samples.len() < 100_000 {
                samples.push(elapsed.as_secs_f64() * 1000.0);
            }
        },
        {
            let profiler = Arc::clone(&profiler);
            move |global| {
                let now = std::time::Instant::now();
                global
                    .timings
                    .get_or_insert_with(|| Arc::clone(profiler.timings()));
                GlobalRegion::tick(global);
                profiler.record_global(now.elapsed());
            }
        },
    );
    let (signal_tx, mut signals) = mpsc::unbounded_channel();
    let mut mailboxes: HashMap<Uuid, (u64, Mailbox, mpsc::UnboundedSender<Outbound>)> =
        HashMap::new();
    let mut pending: HashMap<Uuid, GameMessage> = HashMap::new();
    let mut permissions: HashMap<Uuid, u8> = HashMap::new();
    let mut names: HashMap<Uuid, String> = HashMap::new();
    let mut command_tree = commands::dispatcher();
    if let Some(plugins) = &settings.plugins {
        match plugins.plugin_commands() {
            Ok(commands) => commands::register_plugin_commands(&mut command_tree, &commands),
            Err(error) => tracing::warn!(%error, "plugin commands unavailable"),
        }
    }
    let registry_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vanilla/26.3/generated/reports");
    let parser_registry = ferrite_command::ParserRegistry::load(registry_dir).ok();
    let mut holds: HashMap<ChunkPos, usize> = HashMap::new();
    let positions = Arc::new(Mutex::new(HashMap::new()));
    let mut next_entity = 1i32;
    let spawn_pos = [
        spawn[0] as f64 + 0.5,
        spawn[1] as f64,
        spawn[2] as f64 + 0.5,
    ];
    loop {
        tokio::select! {
         event=events.recv()=>{let Some(event)=event else {break}; match event {
          GameMessage::Joined{session,uuid,name,properties,dimension_type,biome,view_distance,online_mode,max_players,outbound}=>{
           if let Some((_, mailbox, old)) = mailboxes.get(&uuid) {
             let _ = old.send(Outbound::Disconnect("You logged in from another location".into()));
             mailbox.lock().unwrap().push_back(Command::Leave);
             if let Some(GameMessage::Joined { outbound: displaced, .. }) = pending.insert(uuid, GameMessage::Joined { session, uuid, name, properties, dimension_type, biome, view_distance, online_mode, max_players, outbound }) { let _ = displaced.send(Outbound::Disconnect("You logged in from another location".into())); }
             continue;
           }
           let permission = settings.permission(uuid);
           // Resume where the player left off; a first join (or unreadable data) uses world spawn.
           let saved = playerdata::load(&uuid);
           let start_pos = saved.as_ref().map_or(spawn_pos, |s| s.pos);
           let (start_yaw, start_pitch) = saved.as_ref().map_or((0.0, 0.0), |s| (s.yaw, s.pitch));
           let start_mode = saved.as_ref().map_or(settings.gamemode, |s| s.gamemode);
           let start_chunk = ChunkPos((start_pos[0].floor() as i32).div_euclid(16), (start_pos[2].floor() as i32).div_euclid(16));
           let start_inventory = saved.as_ref().map_or([ItemStack::default(); 46], |s| s.inventory_array());
           permissions.insert(uuid, permission);
           let entity=next_entity;next_entity+=1;
           emit(&outbound,&PlayLogin{entity_id:entity,hardcore:false,dimensions:vec!["minecraft:overworld".into()],max_players:VarInt(max_players as i32),view_distance:VarInt(view_distance as i32),simulation_distance:VarInt(view_distance as i32),reduced_debug:false,respawn_screen:true,limited_crafting:false,dimension_type:VarInt(dimension_type),dimension:"minecraft:overworld".into(),seed:0,game_mode:VarInt(start_mode),previous_game_mode:VarInt(0),debug:false,flat,death_location:None,portal_cooldown:VarInt(0),sea_level:VarInt(63),online_mode,secure_chat:false});
           let (actions, _) = mpsc::unbounded_channel();
           if let Some(registry) = &parser_registry && let Ok(tree) = command_tree.serialize_commands(&commands::Source { permission, console: false, actions }, registry) {
               let mut packet = Vec::new(); VarInt(16).encode(&mut packet); packet.extend(tree.body); let _ = outbound.send(Outbound::Packet(packet));
           }
           emit(&outbound, &ContainerContent { window:VarInt(0), state:VarInt(0), slots:start_inventory.to_vec(), carried:ItemStack::default() });
           emit(&outbound,&PositionSync{teleport_id:VarInt(1),x:start_pos[0],y:start_pos[1],z:start_pos[2],velocity_x:0.0,velocity_y:0.0,velocity_z:0.0,yaw:start_yaw,pitch:start_pitch,flags:0});
           emit(&outbound,&DefaultSpawn{dimension:"minecraft:overworld".into(),position:ferrite_protocol::types::BlockPos::new(spawn[0],spawn[1],spawn[2]).pack(),yaw:0.0,pitch:0.0});emit(&outbound,&GameEvent{event:13,value:0.0});
           let mailbox=Arc::new(Mutex::new(VecDeque::new()));mailboxes.insert(uuid,(session,mailbox.clone(),outbound.clone()));
           positions.lock().unwrap().insert(uuid, start_pos);
           tracking.lock().unwrap().insert(uuid, (HashSet::new(), outbound.clone()));
           let mut player=Player{uuid,tracking:tracking.clone(),positions:positions.clone(),source:source.clone(),pos:start_pos,yaw:start_yaw,pitch:start_pitch,ground:true,chunk:start_chunk,mailbox,outbound:outbound.clone(),signals:signal_tx.clone(),biome,gamemode:start_mode,inventory:start_inventory,held_slot:0,breaking:None,view:view_distance as i32,pending_teleport:Some(1),next_teleport:2,loaded:HashSet::new(),queue:VecDeque::new(),awaiting_batch:false,rate:4.0};player.refresh_chunks();
           scheduler.add_chunk(player.chunk);scheduler.schedule_at_chunk(player.chunk,move |r|{r.hold_chunk(player.chunk);r.players.insert(uuid,player);});
           names.insert(uuid, name.clone());
           if let Some(plugins) = &settings.plugins && let Err(error) = plugins.player_joined(uuid, &name) { tracing::warn!(%error, "player join event failed"); }
           let mode = start_mode; scheduler.schedule_global(move |g|{let mut player = GlobalPlayer::new(uuid,entity,name,properties,outbound); player.pos = start_pos; player.game_mode = mode; g.join(player)});tracing::info!(%uuid,"player joined");
          },
          GameMessage::Input{session,uuid,packet:ServerboundPlay::Chat(text)}=>{if mailboxes.get(&uuid).is_some_and(|m| m.0 == session) && text.chars().count()<=256 {
              let mut message = Some(text);
              if let Some(plugins) = &settings.plugins {
                  let name = names.get(&uuid).cloned().unwrap_or_default();
                  match plugins.chat(uuid, &name, message.as_deref().unwrap_or_default()) {
                      Ok(allowed) => message = allowed,
                      Err(error) => tracing::warn!(%error, "chat event failed"),
                  }
              }
              if let Some(text) = message { scheduler.schedule_global(move |g|g.chat(uuid,&text)); }
          }},
          GameMessage::Input{session,uuid,packet:ServerboundPlay::Command(command)}=>{if let Some((_, mailbox, outbound)) = mailboxes.get(&uuid).filter(|m| m.0 == session) {
              let (actions, mut received) = mpsc::unbounded_channel();
              let source = commands::Source { permission: *permissions.get(&uuid).unwrap_or(&0), console: false, actions };
              if let Err(error) = command_tree.execute(command.trim_start_matches('/'), source) { emit(outbound, &SystemChat { content:ferrite_protocol::types::PlainText(error.to_string()), overlay:false }); }
              while let Ok(action) = received.try_recv() { match action {
                  commands::Action::GameMode(mode) => { mailbox.lock().unwrap().push_back(Command::SetMode(mode)); scheduler.schedule_global(move |g| g.set_mode(uuid, mode)); },
                  commands::Action::Teleport(pos) => mailbox.lock().unwrap().push_back(Command::Teleport(pos)),
                  commands::Action::Say(text) => scheduler.schedule_global(move |g| g.say(&text)),
                  commands::Action::Tps => for line in profiler.tps_lines() { chat(outbound, &line); },
                  commands::Action::Mspt => for line in profiler.mspt_lines() { chat(outbound, &line); },
                  commands::Action::ProfileStart => match profiler.start_profile() {
                      Ok(()) => chat(outbound, "Profiling started; run /profile stop to dump the slowest systems"),
                      Err(error) => chat(outbound, &format!("Cannot start profiling: {error}")),
                  },
                  commands::Action::ProfileStop => match profiler.stop_profile() {
                      Ok(report) => for line in report.lines() { chat(outbound, &line); },
                      Err(error) => chat(outbound, &format!("Cannot stop profiling: {error}")),
                  },
                  commands::Action::Plugin { name, args } => {
                      if let Some(plugins) = &settings.plugins {
                          let line = plugin_command_line(&name, &args);
                          match plugins.run_plugin_command(ferrite_plugin::CommandSender::Player(uuid), &line) {
                              Ok(result) => for message in result.messages {
                                  if message.player == uuid { chat(outbound, &plugin_message_text(&message.content)); }
                              },
                              Err(error) => tracing::warn!(%error, "plugin command failed"),
                          }
                      }
                  },
                  commands::Action::Stop => {},
              }}
          }},
          GameMessage::Input{session,uuid,packet}=>{if let Some((_,m,_))=mailboxes.get(&uuid).filter(|m| m.0 == session){m.lock().unwrap().push_back(Command::Input(packet));}},
          GameMessage::Left{session,uuid}=>{if matches!(pending.get(&uuid), Some(GameMessage::Joined { session: id, .. }) if *id == session) { pending.remove(&uuid); }
            if let Some((_,m,_))=mailboxes.get(&uuid).filter(|m| m.0 == session){m.lock().unwrap().push_back(Command::Leave);}},
          GameMessage::Console{command,complete}=>{
              let (actions, mut received) = mpsc::unbounded_channel();
              let result = command_tree.execute(command.trim().trim_start_matches('/'), commands::Source { permission: 4, console: true, actions }).map_err(|e| e.to_string());
              let mut stop = false;
              while let Ok(action) = received.try_recv() { match action {
                  commands::Action::Say(text) => scheduler.schedule_global(move |g| g.say(&text)),
                  commands::Action::Stop => stop = true,
                  commands::Action::Tps => for line in profiler.tps_lines_plain() { tracing::info!("{line}"); },
                  commands::Action::Mspt => for line in profiler.mspt_lines_plain() { tracing::info!("{line}"); },
                  commands::Action::ProfileStart => if let Err(error) = profiler.start_profile() { tracing::warn!(%error, "profile start failed"); } else { tracing::info!("profiling started; run profile stop to dump the slowest systems"); },
                  commands::Action::ProfileStop => match profiler.stop_profile() {
                      Ok(report) => for line in report.lines() { tracing::info!("{line}"); },
                      Err(error) => tracing::warn!(%error, "profile stop failed"),
                  },
                  commands::Action::Plugin { name, args } => {
                      if let Some(plugins) = &settings.plugins {
                          let line = plugin_command_line(&name, &args);
                          match plugins.run_plugin_command(ferrite_plugin::CommandSender::Console, &line) {
                              Ok(result) => for message in result.messages { tracing::info!("{}", plugin_message_text(&message.content)); },
                              Err(error) => tracing::warn!(%error, "plugin command failed"),
                          }
                      }
                  },
                  _ => tracing::warn!("command requires a player"),
              }}
              let _ = complete.send(result.map(|_| stop));
          },
          GameMessage::Shutdown{complete}=>{let (ack_tx,ack_rx)=oneshot::channel();scheduler.schedule_global(move |g|{g.shutdown();let _=ack_tx.send(());});let _=ack_rx.await;let _stopped = scheduler.shutdown();let result = if let Some(source) = &source { source.flush_all().await.map_err(|error| format!("failed to save world chunks during shutdown: {error:#}")) } else { Ok(()) }; let _=complete.send(result);break;}
         }},
         signal=signals.recv()=>{let Some(signal)=signal else {break}; match signal {
          Signal::Hold{chunk,add}=>if add{*holds.entry(chunk).or_default() += 1;scheduler.add_chunk(chunk);scheduler.schedule_at_chunk(chunk,move |r|r.hold_chunk(chunk));}else{release_hold(&scheduler, &mut holds, &signal_tx, chunk);},
          Signal::FinishRelease{chunk,flush}=>{scheduler.remove_chunk(chunk);if flush && let Some(source)=source.clone(){tokio::spawn(async move {if let Err(error)=source.flush_chunk(chunk.0,chunk.1).await{tracing::error!(%error,"failed to save unloaded chunk");}});}},
          Signal::FluidChange{x,y,z}=>{
            for (dx,dy,dz) in [(0,0,0),(1,0,0),(-1,0,0),(0,1,0),(0,-1,0),(0,0,1),(0,0,-1)] {
              let (Some(x),Some(y),Some(z))=(x.checked_add(dx),y.checked_add(dy),z.checked_add(dz)) else {continue;};
              let chunk=ChunkPos(x.div_euclid(16),z.div_euclid(16));
              if scheduler.owner(chunk).is_some(){scheduler.schedule_at_chunk(chunk,move |r|r.schedule_fluid((x,y,z)));}
            }
          },
          Signal::Update{uuid,pos,yaw,pitch,ground}=>scheduler.schedule_global(move |g|g.update(uuid,pos,yaw,pitch,ground)),
          Signal::Transfer{from,to,player}=>{scheduler.add_chunk(to);let id=player.uuid;scheduler.schedule_at_chunk(to,move |r|{r.hold_chunk(to);r.players.insert(id,*player);});defer_release(&scheduler,&signal_tx,from,false);},
          Signal::Leave{uuid,chunk,held}=>{tracking.lock().unwrap().remove(&uuid);positions.lock().unwrap().remove(&uuid);permissions.remove(&uuid);mailboxes.remove(&uuid);for pos in held {release_hold(&scheduler, &mut holds, &signal_tx, pos);}defer_release(&scheduler,&signal_tx,chunk,false);
            let name = names.remove(&uuid).unwrap_or_default();
            if let Some(plugins) = &settings.plugins && let Err(error) = plugins.player_left(uuid, &name) { tracing::warn!(%error, "player quit event failed"); }
            scheduler.schedule_global(move |g|g.leave(uuid));if let Some(join)=pending.remove(&uuid){let _=event_tx.send(join);}}
         }}
        }
    }
}
fn defer_release(
    scheduler: &Scheduler<Region, GlobalRegion>,
    signals: &mpsc::UnboundedSender<Signal>,
    chunk: ChunkPos,
    flush: bool,
) {
    let signals = signals.clone();
    scheduler.schedule_at_chunk(chunk, move |region| {
        region.release_chunk(chunk);
        let _ = signals.send(Signal::FinishRelease { chunk, flush });
    });
}
fn release_hold(
    scheduler: &Scheduler<Region, GlobalRegion>,
    holds: &mut HashMap<ChunkPos, usize>,
    signals: &mpsc::UnboundedSender<Signal>,
    chunk: ChunkPos,
) {
    let mut flush = false;
    if let Some(count) = holds.get_mut(&chunk) {
        *count -= 1;
        if *count == 0 {
            holds.remove(&chunk);
            flush = true;
        }
    }
    defer_release(scheduler, signals, chunk, flush);
}
#[cfg(test)]
mod integration;

#[cfg(test)]
mod anvil_tests {
    #[test]
    fn fixture_chunk_encodes_stored_lighting() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let registry =
            ferrite_world::RuntimeRegistry::load(root.join("vanilla/26.3/generated")).unwrap();
        let path = root.join("vanilla/26.3/fixture-world/world");
        let packet = super::anvil::read_chunk(&path, &registry, 0, 0)
            .unwrap()
            .unwrap();
        assert!(!packet.is_empty());
    }
}

#[test]
fn fixture_spawn_comes_from_level_dat() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vanilla/26.3/fixture-world/world");
    assert_eq!(anvil::read_spawn(&root).unwrap(), [0, 80, 112]);
}

#[tokio::test]
async fn modified_chunk_is_written_on_flush() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root
        .join("vanilla/26.3/fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca");
    let temp = std::env::temp_dir().join(format!("ferrite-save-{}", uuid::Uuid::new_v4()));
    let dir = temp.join("dimensions/minecraft/overworld/region");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(fixture, dir.join("r.0.0.mca")).unwrap();
    let registry =
        ferrite_world::RuntimeRegistry::load(root.join("vanilla/26.3/generated")).unwrap();
    let region = ferrite_world::RegionFile::open(dir.join("r.0.0.mca"), 0, 0).unwrap();
    let data = region.read_chunk(0, 7).unwrap().unwrap();
    let (_, tag) = ferrite_nbt::binary::read_named(&data).unwrap();
    let mut chunk = ferrite_world::chunk_from_nbt(&tag, -64, 384, &registry).unwrap();
    use ferrite_world::BlockStateRegistry;
    let air = registry
        .block_id("minecraft:air", &std::collections::BTreeMap::new())
        .unwrap();
    let stone = registry
        .block_id("minecraft:stone", &std::collections::BTreeMap::new())
        .unwrap();
    let value = if chunk.block(0, 68, 0) == Some(air) {
        stone
    } else {
        air
    };
    chunk.set_block(0, 68, 0, value, &registry).unwrap();
    let source = anvil::AnvilChunkSource::new(temp.clone(), registry);
    let mut light = ferrite_world::LightData {
        sky: vec![ferrite_world::LightSection::Unknown; 26],
        block: vec![ferrite_world::LightSection::Unknown; 26],
    };
    light.block[9] = ferrite_world::LightSection::Data(Box::new([0x11; 2048]));
    source.mark_modified(0, 7, chunk, &light).unwrap();
    let world = World::start_with(Some(source), [0, 68, 112]);
    world.shutdown().await.unwrap();
    let data = region.read_chunk(0, 7).unwrap().unwrap();
    let (_, tag) = ferrite_nbt::binary::read_named(&data).unwrap();
    let ferrite_nbt::Tag::Compound(ref root_tag) = tag else {
        panic!("chunk root")
    };
    let Some(ferrite_nbt::Tag::List(ferrite_nbt::List::Compound(sections))) =
        root_tag.get("sections")
    else {
        panic!("sections")
    };
    let section = sections
        .iter()
        .find(|s| s.get("Y") == Some(&ferrite_nbt::Tag::Byte(4)))
        .unwrap();
    assert_eq!(
        section.get("BlockLight"),
        Some(&ferrite_nbt::Tag::ByteArray(vec![0x11; 2048]))
    );
    let registry =
        ferrite_world::RuntimeRegistry::load(root.join("vanilla/26.3/generated")).unwrap();
    let chunk = ferrite_world::chunk_from_nbt(&tag, -64, 384, &registry).unwrap();
    assert_eq!(chunk.block(0, 68, 0), Some(value));
    std::fs::remove_dir_all(temp).unwrap();
}
#[test]
fn operator_permissions_require_the_configured_uuid() {
    let settings = WorldSettings {
        gamemode: 1,
        ops: vec![Operator {
            uuid: Uuid::from_u128(10),
            level: 3,
        }],
        plugins: None,
    };
    assert_eq!(settings.permission(Uuid::from_u128(11)), 0);
    assert_eq!(settings.permission(Uuid::from_u128(10)), 3);
}

#[tokio::test]
async fn memory_chunk_block_mutation_is_visible() {
    let source = anvil::AnvilChunkSource::new_memory();
    while source.get(0, 0, 0).is_none() {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let before = source.block(0, 64, 0).unwrap();
    assert_ne!(before, 0);
    source.set_block(0, 64, 0, 0).unwrap();
    assert_eq!(source.block(0, 64, 0), Some(0));
}
