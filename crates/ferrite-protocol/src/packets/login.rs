use crate::{Bound, Decode, DecodeError, Encode, Packet, RestBytes, State, VarInt};
use uuid::Uuid;

macro_rules! packet {
    ($name:ident, $id:expr, $bound:ident, {$($field:ident: $ty:ty),* $(,)?}) => {
        #[derive(Debug, Clone, PartialEq)]
        pub struct $name { $(pub $field: $ty),* }
        impl Encode for $name { fn encode(&self, out: &mut Vec<u8>) { $(self.$field.encode(out);)* } }
        impl Decode for $name { fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> { Ok(Self { $($field: <$ty>::decode(buf)?),* }) } }
        impl Packet for $name { const ID: i32 = $id; const STATE: State = State::Login; const BOUND: Bound = Bound::$bound; }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}
impl Encode for Property {
    fn encode(&self, out: &mut Vec<u8>) {
        self.name.encode(out);
        self.value.encode(out);
        self.signature.encode(out);
    }
}
impl Decode for Property {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            name: String::decode(buf)?,
            value: String::decode(buf)?,
            signature: Option::<String>::decode(buf)?,
        })
    }
}
packet!(LoginStart, 0, Serverbound, {name: String, uuid: Uuid});
packet!(EncryptionRequest, 1, Clientbound, {server_id: String, public_key: Vec<u8>, verify_token: Vec<u8>, should_authenticate: bool});
packet!(EncryptionResponse, 1, Serverbound, {shared_secret: Vec<u8>, verify_token: Vec<u8>});
packet!(LoginSuccess, 2, Clientbound, {uuid: Uuid, name: String, properties: Vec<Property>, session_id: Uuid});
packet!(SetCompression, 3, Clientbound, {threshold: VarInt});
packet!(LoginDisconnect, 0, Clientbound, {reason: String});
packet!(CustomQueryAnswer, 2, Serverbound, {message_id: VarInt, data: Option<RestBytes>});
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginAcknowledged;
impl Encode for LoginAcknowledged {
    fn encode(&self, _: &mut Vec<u8>) {}
}
impl Decode for LoginAcknowledged {
    fn decode(_: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self)
    }
}
impl Packet for LoginAcknowledged {
    const ID: i32 = 3;
    const STATE: State = State::Login;
    const BOUND: Bound = Bound::Serverbound;
}
