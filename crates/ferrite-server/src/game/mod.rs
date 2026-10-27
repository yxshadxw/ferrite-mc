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
}
impl World {
    pub fn start() -> Self {
        let (sender, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(rx));
        Self { sender, task }
    }
    pub async fn shutdown(self) {
        let (complete, done) = oneshot::channel();
        let _ = self.sender.send(GameMessage::Shutdown { complete });
        let _ = done.await;
        let _ = self.task.await;
    }
}
fn emit<P: ferrite_protocol::Packet>(tx: &mpsc::UnboundedSender<Outbound>, p: &P) {
    let _ = tx.send(game::packet(p));
}
async fn run(mut events: mpsc::UnboundedReceiver<GameMessage>) {
    let scheduler = Scheduler::with_default_threads(
        Regionizer::<Region>::default(),
        GlobalRegion::default(),
        Region::tick,
        GlobalRegion::tick,
    );
    let (signal_tx, mut signals) = mpsc::unbounded_channel();
    let mut mailboxes: HashMap<Uuid, Mailbox> = HashMap::new();
    let mut next_entity = 1i32;
    loop {
        tokio::select! {
         event=events.recv()=>{let Some(event)=event else {break}; match event {
          GameMessage::Joined{uuid,name,properties,dimension_type,biome,view_distance,online_mode,max_players,outbound}=>{
           if mailboxes.contains_key(&uuid){let _=outbound.send(Outbound::Disconnect("Already connected".into()));continue;}
           let entity=next_entity;next_entity+=1;
           emit(&outbound,&PlayLogin{entity_id:entity,hardcore:false,dimensions:vec!["minecraft:overworld".into()],max_players:VarInt(max_players as i32),view_distance:VarInt(view_distance as i32),simulation_distance:VarInt(view_distance as i32),reduced_debug:false,respawn_screen:true,limited_crafting:false,dimension_type:VarInt(dimension_type),dimension:"minecraft:overworld".into(),seed:0,game_mode:VarInt(0),previous_game_mode:VarInt(0),debug:false,flat:true,death_location:None,portal_cooldown:VarInt(0),sea_level:VarInt(63),online_mode,secure_chat:false});
           emit(&outbound,&PositionSync{teleport_id:VarInt(1),x:0.5,y:65.0,z:0.5,velocity_x:0.0,velocity_y:0.0,velocity_z:0.0,yaw:0.0,pitch:0.0,flags:0});
           emit(&outbound,&DefaultSpawn{dimension:"minecraft:overworld".into(),position:ferrite_protocol::types::BlockPos::new(0,65,0).pack(),yaw:0.0,pitch:0.0});emit(&outbound,&GameEvent{event:13,value:0.0});
           let mailbox=Arc::new(Mutex::new(VecDeque::new()));mailboxes.insert(uuid,mailbox.clone());
           let mut player=Player{uuid,pos:[0.5,65.0,0.5],yaw:0.0,pitch:0.0,ground:true,chunk:ChunkPos(0,0),mailbox,outbound:outbound.clone(),signals:signal_tx.clone(),biome,view:view_distance as i32,pending_teleport:Some(1),next_teleport:2,loaded:HashSet::new(),queue:VecDeque::new(),awaiting_batch:false,rate:4.0};player.refresh_chunks();
           scheduler.add_chunk(player.chunk);scheduler.schedule_at_chunk(player.chunk,move |r|{r.players.insert(uuid,player);});
           scheduler.schedule_global(move |g|g.join(GlobalPlayer::new(uuid,entity,name,properties,outbound)));tracing::info!(%uuid,"player joined");
          },
          GameMessage::Input{uuid,packet:ServerboundPlay::Chat(text)}=>{if text.chars().count()<=256 {scheduler.schedule_global(move |g|g.chat(uuid,&text));}},
          GameMessage::Input{uuid,packet:ServerboundPlay::Command(command)}=>tracing::info!(%uuid,%command,"player command"),
          GameMessage::Input{uuid,packet}=>{if let Some(m)=mailboxes.get(&uuid){m.lock().unwrap().push_back(Command::Input(packet));}},
          GameMessage::Left{uuid}=>{if let Some(m)=mailboxes.get(&uuid){m.lock().unwrap().push_back(Command::Leave);}},
          GameMessage::Shutdown{complete}=>{let (ack_tx,ack_rx)=oneshot::channel();scheduler.schedule_global(move |g|{g.shutdown();let _=ack_tx.send(());});let _=ack_rx.await;tokio::time::sleep(std::time::Duration::from_millis(100)).await;let _=complete.send(());break;}
         }},
         signal=signals.recv()=>{let Some(signal)=signal else {break}; match signal {
          Signal::Hold{chunk,add}=>if add{scheduler.add_chunk(chunk);}else{scheduler.remove_chunk(chunk);},
          Signal::Update{uuid,pos,yaw,pitch,ground}=>scheduler.schedule_global(move |g|g.update(uuid,pos,yaw,pitch,ground)),
          Signal::Transfer{from,to,player}=>{scheduler.add_chunk(to);let id=player.uuid;scheduler.schedule_at_chunk(to,move |r|{r.players.insert(id,player);});scheduler.remove_chunk(from);},
          Signal::Leave{uuid,chunk,held}=>{mailboxes.remove(&uuid);for pos in held {scheduler.remove_chunk(pos);}scheduler.remove_chunk(chunk);scheduler.schedule_global(move |g|g.leave(uuid));}
         }}
        }
    }
}
#[cfg(test)]
mod integration;
