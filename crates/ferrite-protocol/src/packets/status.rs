use crate::{Bound, Decode, DecodeError, Encode, Packet, State};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRequest;
impl Encode for StatusRequest {
    fn encode(&self, _: &mut Vec<u8>) {}
}
impl Decode for StatusRequest {
    fn decode(_: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self)
    }
}
impl Packet for StatusRequest {
    const ID: i32 = 0;
    const STATE: State = State::Status;
    const BOUND: Bound = Bound::Serverbound;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusResponse {
    pub json: String,
}
impl Encode for StatusResponse {
    fn encode(&self, out: &mut Vec<u8>) {
        self.json.encode(out);
    }
}
impl Decode for StatusResponse {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            json: String::decode(buf)?,
        })
    }
}
impl Packet for StatusResponse {
    const ID: i32 = 0;
    const STATE: State = State::Status;
    const BOUND: Bound = Bound::Clientbound;
}

macro_rules! ping_packet {
    ($name:ident, $bound:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            pub payload: i64,
        }
        impl Encode for $name {
            fn encode(&self, out: &mut Vec<u8>) {
                self.payload.encode(out);
            }
        }
        impl Decode for $name {
            fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
                Ok(Self {
                    payload: i64::decode(buf)?,
                })
            }
        }
        impl Packet for $name {
            const ID: i32 = 1;
            const STATE: State = State::Status;
            const BOUND: Bound = Bound::$bound;
        }
    };
}
ping_packet!(PingRequest, Serverbound);
ping_packet!(PongResponse, Clientbound);

#[cfg(test)]
mod tests {
    use super::*;
    fn roundtrip<T: Encode + Decode + PartialEq + std::fmt::Debug>(value: T) {
        let mut bytes = Vec::new();
        value.encode(&mut bytes);
        let mut input = bytes.as_slice();
        assert_eq!(T::decode(&mut input).unwrap(), value);
        assert!(input.is_empty());
    }
    #[test]
    fn status_packets_roundtrip_and_ids() {
        roundtrip(StatusRequest);
        roundtrip(StatusResponse {
            json: "{\"version\":{}}".into(),
        });
        roundtrip(PingRequest { payload: i64::MIN });
        roundtrip(PongResponse { payload: i64::MAX });
        assert_eq!(
            (
                StatusRequest::ID,
                StatusResponse::ID,
                PingRequest::ID,
                PongResponse::ID
            ),
            (0, 0, 1, 1)
        );
    }
}
