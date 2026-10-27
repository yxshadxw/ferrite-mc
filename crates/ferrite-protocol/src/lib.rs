//! Minecraft Java Edition wire protocol: primitive codecs, framing, and packet definitions.
//!
//! The core speaks exactly one protocol version ([`PROTOCOL_VERSION`]); other versions are
//! handled by a translation layer in front of it.

pub mod codec;
pub mod frame;
pub mod packets;
pub mod types;

pub use codec::{Decode, DecodeError, Encode, RestBytes, VarInt, VarLong};
pub use frame::{FrameDecoder, FrameEncoder};

/// Minecraft Java 26.3.
pub const PROTOCOL_VERSION: i32 = 777;
pub const GAME_VERSION: &str = "26.3";

/// Connection state, which selects the packet ID namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    Handshaking,
    Status,
    Login,
    Configuration,
    Play,
}

/// Direction a packet travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bound {
    Serverbound,
    Clientbound,
}

/// A packet with a fixed ID in a fixed state and direction.
pub trait Packet: Encode + Decode {
    const ID: i32;
    const STATE: State;
    const BOUND: Bound;
}

/// Encode a packet body (ID + fields) into `out`, without the length prefix.
pub fn encode_packet<P: Packet>(packet: &P, out: &mut Vec<u8>) {
    VarInt(P::ID).encode(out);
    packet.encode(out);
}
