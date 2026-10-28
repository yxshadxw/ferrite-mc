mod anvil;
pub use anvil::{AnvilChunkSource, read_spawn as load_spawn};
pub fn new_anvil_source(
    path: std::path::PathBuf,
    registry: ferrite_world::RuntimeRegistry,
) -> Arc<AnvilChunkSource> {
    AnvilChunkSource::new(path, registry)
}
mod chunk;
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
mod global;
mod region;

use self::{
    global::{GlobalPlayer, GlobalRegion},
    region::{Command, Mailbox, Player, Region, Signal},
};
use ferrite_net::game::{self, GameMessage, Outbound, ServerboundPlay};
use ferrite_protocol::{VarInt, packets::play::*};
use ferrite_region::{ChunkPos, Regionizer, Scheduler};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;
pub struct World {
    pub sender: mpsc::UnboundedSender<GameMessage>,
    task: tokio::task::JoinHandle<()>,
    pub tick_samples: Arc<Mutex<Vec<f64>>>,
}
impl World {
    pub fn start() -> Self {
        Self::start_with(None, [0, 65, 0])
    }
    pub fn start_with(source: Option<Arc<anvil::AnvilChunkSource>>, spawn: [i32; 3]) -> Self {
        let (sender, rx) = mpsc::unbounded_channel();
        let tick_samples = Arc::new(Mutex::new(Vec::new()));
        let task = tokio::spawn(run(rx, sender.clone(), source, spawn, tick_samples.clone()));
        Self {
            sender,
            task,
            tick_samples,
        }
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
async fn run(
    mut events: mpsc::UnboundedReceiver<GameMessage>,
    event_tx: mpsc::UnboundedSender<GameMessage>,
    source: Option<Arc<anvil::AnvilChunkSource>>,
    spawn: [i32; 3],
    tick_samples: Arc<Mutex<Vec<f64>>>,
) {
    let scheduler = Scheduler::with_default_threads(
        Regionizer::<Region>::default(),
        GlobalRegion::default(),
        move |region| {
            let now = std::time::Instant::now();
            Region::tick(region);
            let mut samples = tick_samples.lock().unwrap();
            if samples.len() < 100_000 {
                samples.push(now.elapsed().as_secs_f64() * 1000.0);
            }
        },
        GlobalRegion::tick,
    );
    let (signal_tx, mut signals) = mpsc::unbounded_channel();
    let mut mailboxes: HashMap<Uuid, (u64, Mailbox, mpsc::UnboundedSender<Outbound>)> =
        HashMap::new();
    let mut pending: HashMap<Uuid, GameMessage> = HashMap::new();
    let mut holds: HashMap<ChunkPos, usize> = HashMap::new();
    let mut next_entity = 1i32;
    let spawn_pos = [
        spawn[0] as f64 + 0.5,
        spawn[1] as f64,
        spawn[2] as f64 + 0.5,
    ];
    let spawn_chunk = ChunkPos(spawn[0].div_euclid(16), spawn[2].div_euclid(16));
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
           let entity=next_entity;next_entity+=1;
           emit(&outbound,&PlayLogin{entity_id:entity,hardcore:false,dimensions:vec!["minecraft:overworld".into()],max_players:VarInt(max_players as i32),view_distance:VarInt(view_distance as i32),simulation_distance:VarInt(view_distance as i32),reduced_debug:false,respawn_screen:true,limited_crafting:false,dimension_type:VarInt(dimension_type),dimension:"minecraft:overworld".into(),seed:0,game_mode:VarInt(0),previous_game_mode:VarInt(0),debug:false,flat:source.is_none(),death_location:None,portal_cooldown:VarInt(0),sea_level:VarInt(63),online_mode,secure_chat:false});
           emit(&outbound,&PositionSync{teleport_id:VarInt(1),x:spawn_pos[0],y:spawn_pos[1],z:spawn_pos[2],velocity_x:0.0,velocity_y:0.0,velocity_z:0.0,yaw:0.0,pitch:0.0,flags:0});
           emit(&outbound,&DefaultSpawn{dimension:"minecraft:overworld".into(),position:ferrite_protocol::types::BlockPos::new(spawn[0],spawn[1],spawn[2]).pack(),yaw:0.0,pitch:0.0});emit(&outbound,&GameEvent{event:13,value:0.0});
           let mailbox=Arc::new(Mutex::new(VecDeque::new()));mailboxes.insert(uuid,(session,mailbox.clone(),outbound.clone()));
           let mut player=Player{uuid,source:source.clone(),pos:spawn_pos,yaw:0.0,pitch:0.0,ground:true,chunk:spawn_chunk,mailbox,outbound:outbound.clone(),signals:signal_tx.clone(),biome,view:view_distance as i32,pending_teleport:Some(1),next_teleport:2,loaded:HashSet::new(),queue:VecDeque::new(),awaiting_batch:false,rate:4.0};player.refresh_chunks();
           scheduler.add_chunk(player.chunk);scheduler.schedule_at_chunk(player.chunk,move |r|{r.players.insert(uuid,player);});
           scheduler.schedule_global(move |g|{let mut player = GlobalPlayer::new(uuid,entity,name,properties,outbound); player.pos = spawn_pos; g.join(player)});tracing::info!(%uuid,"player joined");
          },
          GameMessage::Input{session,uuid,packet:ServerboundPlay::Chat(text)}=>{if mailboxes.get(&uuid).is_some_and(|m| m.0 == session) && text.chars().count()<=256 {scheduler.schedule_global(move |g|g.chat(uuid,&text));}},
          GameMessage::Input{session,uuid,packet:ServerboundPlay::Command(command)}=>{if mailboxes.get(&uuid).is_some_and(|m| m.0 == session) {tracing::info!(%uuid,%command,"player command");}},
          GameMessage::Input{session,uuid,packet}=>{if let Some((_,m,_))=mailboxes.get(&uuid).filter(|m| m.0 == session){m.lock().unwrap().push_back(Command::Input(packet));}},
          GameMessage::Left{session,uuid}=>{if matches!(pending.get(&uuid), Some(GameMessage::Joined { session: id, .. }) if *id == session) { pending.remove(&uuid); }
            if let Some((_,m,_))=mailboxes.get(&uuid).filter(|m| m.0 == session){m.lock().unwrap().push_back(Command::Leave);}},
          GameMessage::Shutdown{complete}=>{let (ack_tx,ack_rx)=oneshot::channel();scheduler.schedule_global(move |g|{g.shutdown();let _=ack_tx.send(());});let _=ack_rx.await;let result = if let Some(source) = &source { source.flush_all().await.map_err(|error| format!("failed to save world chunks during shutdown: {error:#}")) } else { Ok(()) }; tokio::time::sleep(std::time::Duration::from_millis(100)).await;let _=complete.send(result);break;}
         }},
         signal=signals.recv()=>{let Some(signal)=signal else {break}; match signal {
          Signal::Hold{chunk,add}=>if add{*holds.entry(chunk).or_default() += 1;scheduler.add_chunk(chunk);}else{release_hold(&scheduler, &mut holds, &source, chunk);},
          Signal::Update{uuid,pos,yaw,pitch,ground}=>scheduler.schedule_global(move |g|g.update(uuid,pos,yaw,pitch,ground)),
          Signal::Transfer{from,to,player}=>{scheduler.add_chunk(to);let id=player.uuid;scheduler.schedule_at_chunk(to,move |r|{r.players.insert(id,player);});scheduler.remove_chunk(from);},
          Signal::Leave{uuid,chunk,held}=>{mailboxes.remove(&uuid);for pos in held {release_hold(&scheduler, &mut holds, &source, pos);}scheduler.remove_chunk(chunk);scheduler.schedule_global(move |g|g.leave(uuid));if let Some(join)=pending.remove(&uuid){let _=event_tx.send(join);}}
         }}
        }
    }
}
fn release_hold(
    scheduler: &Scheduler<Region, GlobalRegion>,
    holds: &mut HashMap<ChunkPos, usize>,
    source: &Option<Arc<anvil::AnvilChunkSource>>,
    chunk: ChunkPos,
) {
    scheduler.remove_chunk(chunk);
    if let Some(count) = holds.get_mut(&chunk) {
        *count -= 1;
        if *count == 0 {
            holds.remove(&chunk);
            if let Some(source) = source.clone() {
                tokio::spawn(async move {
                    if let Err(error) = source.flush_chunk(chunk.0, chunk.1).await {
                        tracing::error!(%error, "failed to save unloaded chunk");
                    }
                });
            }
        }
    }
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
    assert_eq!(anvil::read_spawn(&root).unwrap(), [0, 68, 112]);
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
