use ferrite_net::game::{Outbound, packet};
use ferrite_protocol::{
    VarInt,
    packets::{login::Property, play::*},
    types::PlainText,
};
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;
#[derive(Clone)]
pub struct GlobalPlayer {
    pub uuid: Uuid,
    pub entity: i32,
    pub name: String,
    pub properties: Vec<Property>,
    pub game_mode: i32,
    pub outbound: UnboundedSender<Outbound>,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub ground: bool,
}
impl GlobalPlayer {
    pub fn new(
        uuid: Uuid,
        entity: i32,
        name: String,
        properties: Vec<Property>,
        outbound: UnboundedSender<Outbound>,
    ) -> Self {
        Self {
            uuid,
            entity,
            name,
            properties,
            game_mode: 1,
            outbound,
            pos: [0.5, 65.0, 0.5],
            yaw: 0.0,
            pitch: 0.0,
            ground: true,
        }
    }
    fn info(&self) -> PlayerInfo {
        PlayerInfo {
            uuid: self.uuid,
            name: self.name.clone(),
            properties: self.properties.clone(),
            game_mode: VarInt(self.game_mode),
        }
    }
}
#[derive(Default)]
pub struct GlobalRegion {
    players: HashMap<Uuid, GlobalPlayer>,
    visible: HashMap<(Uuid, Uuid), GlobalPlayer>,
}
fn send<P: ferrite_protocol::Packet>(to: &GlobalPlayer, p: &P) {
    let _ = to.outbound.send(packet(p));
}
fn angle(v: f32) -> u8 {
    (v * 256.0 / 360.0).floor() as i32 as u8
}
impl GlobalRegion {
    pub fn join(&mut self, p: GlobalPlayer) {
        for old in self.players.values() {
            send(old, &p.info());
            send(&p, &old.info());
        }
        send(&p, &p.info());
        self.broadcast(&format!("{} joined the game", p.name));
        self.players.insert(p.uuid, p);
    }
    pub fn update(&mut self, id: Uuid, pos: [f64; 3], yaw: f32, pitch: f32, ground: bool) {
        if let Some(p) = self.players.get_mut(&id) {
            p.pos = pos;
            p.yaw = yaw;
            p.pitch = pitch;
            p.ground = ground;
        }
    }
    pub fn set_mode(&mut self, id: Uuid, mode: i32) {
        if let Some(player) = self.players.get_mut(&id) {
            player.game_mode = mode;
        }
        let update = PlayerGameMode {
            uuid: id,
            game_mode: VarInt(mode),
        };
        for player in self.players.values() {
            send(player, &update);
        }
    }
    pub fn broadcast_text(&self, message: &str) {
        self.broadcast(message);
    }
    pub fn chat(&mut self, id: Uuid, msg: &str) {
        if let Some(p) = self.players.get(&id) {
            self.broadcast(&format!("<{}> {}", p.name, msg));
        }
    }
    fn broadcast(&self, text: &str) {
        for p in self.players.values() {
            send(
                p,
                &SystemChat {
                    content: PlainText(text.into()),
                    overlay: false,
                },
            );
        }
    }
    pub fn leave(&mut self, id: Uuid) {
        if let Some(p) = self.players.remove(&id) {
            for other in self.players.values() {
                send(other, &PlayerInfoRemove { uuids: vec![id] });
                if self.visible.remove(&(other.uuid, id)).is_some() {
                    send(
                        other,
                        &RemoveEntities {
                            ids: vec![VarInt(p.entity)],
                        },
                    );
                }
            }
            self.visible.retain(|(observer, _), _| *observer != id);
            self.broadcast(&format!("{} left the game", p.name));
        }
    }
    pub fn shutdown(&mut self) {
        for p in self.players.values() {
            let _ = p
                .outbound
                .send(Outbound::Disconnect("Server shutting down".into()));
        }
    }
    pub fn tick(&mut self) {
        let players: Vec<_> = self.players.values().cloned().collect();
        for observer in &players {
            for target in &players {
                if observer.uuid == target.uuid {
                    continue;
                }
                let key = (observer.uuid, target.uuid);
                let distance = observer
                    .pos
                    .iter()
                    .zip(target.pos)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f64>();
                let inside = distance <= 48.0 * 48.0;
                match (self.visible.get(&key), inside) {
                    (None, true) => {
                        send(
                            observer,
                            &AddEntity {
                                id: VarInt(target.entity),
                                uuid: target.uuid,
                                entity_type: VarInt(159),
                                x: target.pos[0],
                                y: target.pos[1],
                                z: target.pos[2],
                                pitch: angle(target.pitch),
                                yaw: angle(target.yaw),
                                head_yaw: angle(target.yaw),
                                data: VarInt(0),
                            },
                        );
                        self.visible.insert(key, target.clone());
                    }
                    (Some(_), false) => {
                        send(
                            observer,
                            &RemoveEntities {
                                ids: vec![VarInt(target.entity)],
                            },
                        );
                        self.visible.remove(&key);
                    }
                    (Some(old), true) => {
                        let delta: Vec<i64> = target
                            .pos
                            .iter()
                            .zip(old.pos)
                            .map(|(n, o)| (n * 4096.0).floor() as i64 - (o * 4096.0).floor() as i64)
                            .collect();
                        let moved = delta.iter().any(|v| *v != 0);
                        let rotated = old.yaw != target.yaw || old.pitch != target.pitch;
                        if moved {
                            if delta.iter().all(|d| i16::try_from(*d).is_ok()) {
                                let dx = delta[0] as i16;
                                let dy = delta[1] as i16;
                                let dz = delta[2] as i16;
                                let properties = VarInt(i32::from(target.ground));
                                if rotated {
                                    send(
                                        observer,
                                        &EntityMoveRotation {
                                            id: VarInt(target.entity),
                                            properties,
                                            dx,
                                            dy,
                                            dz,
                                            yaw: angle(target.yaw),
                                            pitch: angle(target.pitch),
                                        },
                                    );
                                } else {
                                    send(
                                        observer,
                                        &EntityMove {
                                            id: VarInt(target.entity),
                                            properties,
                                            dx,
                                            dy,
                                            dz,
                                        },
                                    );
                                }
                            } else {
                                send(
                                    observer,
                                    &EntityTeleport {
                                        id: VarInt(target.entity),
                                        path_type: VarInt(0),
                                        x: target.pos[0],
                                        y: target.pos[1],
                                        z: target.pos[2],
                                        yaw: target.yaw,
                                        pitch: target.pitch,
                                        on_ground: target.ground,
                                    },
                                );
                            }
                        } else if rotated {
                            send(
                                observer,
                                &EntityRotation {
                                    id: VarInt(target.entity),
                                    on_ground: target.ground,
                                    yaw: angle(target.yaw),
                                    pitch: angle(target.pitch),
                                },
                            );
                        }
                        if rotated {
                            send(
                                observer,
                                &EntityHeadRotation {
                                    id: VarInt(target.entity),
                                    yaw: angle(target.yaw),
                                },
                            );
                        }
                        self.visible.insert(key, target.clone());
                    }
                    _ => {}
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visibility_and_chat_follow_positions() {
        let mut g = GlobalRegion::default();
        let (a, mut ar) = tokio::sync::mpsc::unbounded_channel();
        let (b, mut br) = tokio::sync::mpsc::unbounded_channel();
        let pa = GlobalPlayer::new(uuid::Uuid::from_u128(1), 1, "A".into(), vec![], a);
        let pb = GlobalPlayer::new(uuid::Uuid::from_u128(2), 2, "B".into(), vec![], b);
        g.join(pa);
        g.join(pb);
        g.tick();
        assert!(
            std::iter::from_fn(|| br.try_recv().ok())
                .any(|p| matches!(p,Outbound::Packet(ref b) if b[0]==1))
        );
        g.chat(uuid::Uuid::from_u128(1), "hello");
        assert!(
            std::iter::from_fn(|| br.try_recv().ok())
                .any(|p| matches!(p,Outbound::Packet(ref b) if b[0]==124))
        );
        g.leave(uuid::Uuid::from_u128(1));
        assert!(
            std::iter::from_fn(|| br.try_recv().ok())
                .any(|p| matches!(p,Outbound::Packet(ref b) if b[0]==78))
        );
        let _ = &mut ar;
    }
}
