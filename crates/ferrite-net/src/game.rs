use ferrite_protocol::{
    Decode, DecodeError, Packet, VarInt, encode_packet,
    packets::{login::Property, play::*},
};
use tokio::sync::mpsc;
use uuid::Uuid;
/// Messages decoded from a play connection, with no world state in the transport.
#[derive(Debug, Clone)]
pub enum ServerboundPlay {
    PlayerAction(PlayerAction),
    UseItemOn(UseItemOn),
    SetHeldItem(SetHeldItem),
    CreativeSlot(CreativeSlot<ferrite_item::ItemStack>),
    ContainerClick(ContainerClick<ferrite_item::HashedSlot>),
    ContainerClose(i32),
    Position(MovePosition),
    PositionRotation(MovePositionRotation),
    Rotation(MoveRotation),
    Flags(MoveFlags),
    Confirm(ConfirmTeleport),
    Loaded,
    ChunkBatchReceived(f32),
    /// Boxed: `ServerboundChat` carries the 256-byte signature, which would otherwise dominate
    /// every `GameMessage::Input`.
    Chat(Box<ServerboundChat>),
    ChatAck(ServerboundChatAck),
    ChatCommand(ServerboundChatCommand),
    ChatCommandSigned(ServerboundChatCommandSigned),
    ChatSessionUpdate(ServerboundChatSessionUpdate),
}
#[derive(Debug)]
pub enum GameMessage {
    Joined {
        session: u64,
        uuid: Uuid,
        name: String,
        properties: Vec<Property>,
        /// The forwarded address behind a proxy, otherwise the peer's address.
        address: Option<std::net::IpAddr>,
        dimension_type: i32,
        biome: i32,
        view_distance: u8,
        online_mode: bool,
        /// Whether the client must prove chat messages with profile-key signatures
        /// (`enforce-secure-profile` on an authenticated server).
        secure_chat: bool,
        /// The registry id of `minecraft:chat` in the synchronized `chat_type` registry.
        chat_type: i32,
        /// Client chat visibility: 0 = shown, 1 = commands only, 2 = hidden.
        chat_visibility: u8,
        max_players: u32,
        outbound: mpsc::UnboundedSender<Outbound>,
    },
    Input {
        session: u64,
        uuid: Uuid,
        packet: ServerboundPlay,
    },
    Left {
        session: u64,
        uuid: Uuid,
    },
    Console {
        command: String,
        complete: tokio::sync::oneshot::Sender<Result<bool, String>>,
    },
    Shutdown {
        complete: tokio::sync::oneshot::Sender<Result<(), String>>,
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
        6 => Some(ServerboundPlay::ChatAck(ServerboundChatAck::decode(
            &mut b,
        )?)),
        7 => Some(ServerboundPlay::ChatCommand(
            ServerboundChatCommand::decode(&mut b)?,
        )),
        8 => Some(ServerboundPlay::ChatCommandSigned(
            ServerboundChatCommandSigned::decode(&mut b)?,
        )),
        9 => Some(ServerboundPlay::Chat(Box::new(ServerboundChat::decode(
            &mut b,
        )?))),
        10 => Some(ServerboundPlay::ChatSessionUpdate(
            ServerboundChatSessionUpdate::decode(&mut b)?,
        )),
        11 => Some(ServerboundPlay::ChunkBatchReceived(
            ChunkBatchReceived::decode(&mut b)?.chunks_per_tick,
        )),
        30 => Some(ServerboundPlay::Position(MovePosition::decode(&mut b)?)),
        31 => Some(ServerboundPlay::PositionRotation(
            MovePositionRotation::decode(&mut b)?,
        )),
        32 => Some(ServerboundPlay::Rotation(MoveRotation::decode(&mut b)?)),
        33 => Some(ServerboundPlay::Flags(MoveFlags::decode(&mut b)?)),
        18 => Some(ServerboundPlay::ContainerClick(ContainerClick::decode(
            &mut b,
        )?)),
        19 => Some(ServerboundPlay::ContainerClose(
            ServerboundContainerClose::decode(&mut b)?.window.0,
        )),
        41 => Some(ServerboundPlay::PlayerAction(PlayerAction::decode(&mut b)?)),
        54 => Some(ServerboundPlay::SetHeldItem(SetHeldItem::decode(&mut b)?)),
        57 => Some(ServerboundPlay::CreativeSlot(CreativeSlot::decode(&mut b)?)),
        66 => Some(ServerboundPlay::UseItemOn(UseItemOn::decode(&mut b)?)),
        44 => {
            PlayerLoaded::decode(&mut b)?;
            Some(ServerboundPlay::Loaded)
        }
        _ => None,
    })
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    #[test]
    fn routes_interaction_packets() {
        let mut bytes = Vec::new();
        encode_packet(
            &PlayerAction {
                status: VarInt(0),
                location: 0,
                face: 1,
                sequence: VarInt(3),
            },
            &mut bytes,
        );
        assert!(matches!(
            decode_play(&bytes).unwrap(),
            Some(ServerboundPlay::PlayerAction(_))
        ));
    }
    #[test]
    fn routes_signed_chat_and_acknowledgements() {
        use ferrite_protocol::packets::play::{LastSeenUpdate, MessageSignature};
        let mut bytes = Vec::new();
        encode_packet(
            &ServerboundChat {
                message: "hello".into(),
                timestamp: 1,
                salt: 2,
                signature: Some(MessageSignature([7; 256])),
                last_seen: LastSeenUpdate::new(0, &[1], 0),
            },
            &mut bytes,
        );
        let Some(ServerboundPlay::Chat(chat)) = decode_play(&bytes).unwrap() else {
            panic!("expected chat");
        };
        assert_eq!(chat.message, "hello");
        assert!(chat.signature.is_some());

        let mut bytes = Vec::new();
        encode_packet(&ServerboundChatAck { offset: VarInt(4) }, &mut bytes);
        assert!(matches!(
            decode_play(&bytes).unwrap(),
            Some(ServerboundPlay::ChatAck(ServerboundChatAck {
                offset: VarInt(4)
            }))
        ));
    }
}
