use crate::codec::{Decode, DecodeError, Encode, VarInt, decode_string};
use crate::{Bound, Packet, State};

/// What the client wants after the handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Status,
    Login,
    Transfer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intention {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub intent: Intent,
}

impl Encode for Intention {
    fn encode(&self, out: &mut Vec<u8>) {
        VarInt(self.protocol_version).encode(out);
        self.server_address.encode(out);
        self.server_port.encode(out);
        VarInt(match self.intent {
            Intent::Status => 1,
            Intent::Login => 2,
            Intent::Transfer => 3,
        })
        .encode(out);
    }
}

impl Decode for Intention {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            protocol_version: VarInt::decode(buf)?.0,
            // Bungee/Velocity legacy forwarding packs extra data into the address field.
            server_address: decode_string(buf, 32767)?,
            server_port: u16::decode(buf)?,
            intent: match VarInt::decode(buf)?.0 {
                1 => Intent::Status,
                2 => Intent::Login,
                3 => Intent::Transfer,
                _ => return Err(DecodeError::Invalid("handshake intent")),
            },
        })
    }
}

impl Packet for Intention {
    const ID: i32 = 0x00;
    const STATE: State = State::Handshaking;
    const BOUND: Bound = Bound::Serverbound;
}
