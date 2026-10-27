use ferrite_protocol::{
    Decode, DecodeError, Packet, VarInt, encode_packet,
    packets::{login::Property, play::*},
};
use tokio::sync::mpsc;
use uuid::Uuid;
/// Messages decoded from a play connection, with no world state in the transport.
#[derive(Debug, Clone)]
pub enum ServerboundPlay {
    Position(MovePosition),
    PositionRotation(MovePositionRotation),
    Rotation(MoveRotation),
    Flags(MoveFlags),
    Confirm(ConfirmTeleport),
    Loaded,
    ChunkBatchReceived(f32),
    Chat(String),
    Command(String),
}
#[derive(Debug)]
pub enum GameMessage {
    Joined {
        uuid: Uuid,
        name: String,
        properties: Vec<Property>,
        dimension_type: i32,
        biome: i32,
        view_distance: u8,
        online_mode: bool,
        max_players: u32,
        outbound: mpsc::UnboundedSender<Outbound>,
    },
    Input {
        uuid: Uuid,
        packet: ServerboundPlay,
    },
    Left {
        uuid: Uuid,
    },
    Shutdown {
        complete: tokio::sync::oneshot::Sender<()>,
    },
}
#[derive(Debug)]
pub enum Outbound {
    Packet(Vec<u8>),
    Disconnect(String),
}
pub fn packet<P: Packet>(p: &P) -> Outbound {
    let mut bytes = Vec::new();
    encode_packet(p, &mut bytes);
    Outbound::Packet(bytes)
}
pub fn decode_play(data: &[u8]) -> Result<Option<ServerboundPlay>, DecodeError> {
    let mut b = data;
    let id = VarInt::decode(&mut b)?.0;
    Ok(match id {
        0 => Some(ServerboundPlay::Confirm(ConfirmTeleport::decode(&mut b)?)),
        7 | 8 => Some(ServerboundPlay::Command(String::decode(&mut b)?)),
        9 => Some(ServerboundPlay::Chat(String::decode(&mut b)?)),
        11 => Some(ServerboundPlay::ChunkBatchReceived(
            ChunkBatchReceived::decode(&mut b)?.chunks_per_tick,
        )),
        30 => Some(ServerboundPlay::Position(MovePosition::decode(&mut b)?)),
        31 => Some(ServerboundPlay::PositionRotation(
            MovePositionRotation::decode(&mut b)?,
        )),
        32 => Some(ServerboundPlay::Rotation(MoveRotation::decode(&mut b)?)),
        33 => Some(ServerboundPlay::Flags(MoveFlags::decode(&mut b)?)),
        44 => {
            PlayerLoaded::decode(&mut b)?;
            Some(ServerboundPlay::Loaded)
        }
        _ => None,
    })
}
