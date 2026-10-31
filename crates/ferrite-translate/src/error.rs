use ferrite_protocol::{Bound, DecodeError, State};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TranslateError {
    #[error("decode: {0}")]
    Decode(#[from] DecodeError),
    #[error("unknown {bound:?} packet {id} in state {state:?} for protocol {protocol}")]
    UnknownPacket {
        protocol: i32,
        state: State,
        bound: Bound,
        id: i32,
    },
    #[error("unsupported client protocol {0}")]
    Unsupported(i32),
    #[error("invalid packet: {0}")]
    Invalid(&'static str),
}
