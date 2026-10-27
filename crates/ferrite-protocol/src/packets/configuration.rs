use crate::{Bound, Decode, DecodeError, Encode, Packet, RestBytes, State, VarInt};
macro_rules! fields {
    ($name:ident {$($field:ident: $ty:ty),* $(,)?}) => {
        #[derive(Debug, Clone, PartialEq)] pub struct $name { $(pub $field: $ty),* }
        impl Encode for $name { fn encode(&self, out: &mut Vec<u8>) { $(self.$field.encode(out);)* } }
        impl Decode for $name { fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> { Ok(Self { $($field: <$ty>::decode(buf)?),* }) } }
    }
}
macro_rules! packet { ($name:ident, $id:expr, $bound:ident, {$($field:ident: $ty:ty),* $(,)?}) => {
    fields!($name {$($field: $ty),*}); impl Packet for $name { const ID:i32=$id; const STATE:State=State::Configuration; const BOUND:Bound=Bound::$bound; }
}; }
fields!(KnownPack {
    namespace: String,
    id: String,
    version: String
});
fields!(Tag {name: String, entries: Vec<VarInt>});
fields!(RegistryTags {registry: String, tags: Vec<Tag>});
packet!(ClientboundKnownPacks, 15, Clientbound, {packs: Vec<KnownPack>});
packet!(ServerboundKnownPacks, 7, Serverbound, {packs: Vec<KnownPack>});
packet!(UpdateTags, 14, Clientbound, {registries: Vec<RegistryTags>});
packet!(ClientboundCustomPayload, 1, Clientbound, {channel: String, data: RestBytes});
packet!(ServerboundCustomPayload, 2, Serverbound, {channel: String, data: RestBytes});
packet!(ClientboundKeepAlive, 4, Clientbound, {id: i64});
packet!(ServerboundKeepAlive, 4, Serverbound, {id: i64});
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryData {
    pub registry: String,
    pub entries: Vec<String>,
}
impl Encode for RegistryData {
    fn encode(&self, out: &mut Vec<u8>) {
        self.registry.encode(out);
        VarInt(self.entries.len() as i32).encode(out);
        for e in &self.entries {
            e.encode(out);
            false.encode(out);
        }
    }
}
impl Decode for RegistryData {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let registry = String::decode(buf)?;
        let n = usize::try_from(VarInt::decode(buf)?.0)
            .map_err(|_| DecodeError::Invalid("registry length"))?;
        if n > buf.len() {
            return Err(DecodeError::Invalid("registry length"));
        }
        let mut entries = Vec::with_capacity(n);
        for _ in 0..n {
            entries.push(String::decode(buf)?);
            if bool::decode(buf)? {
                return Err(DecodeError::Invalid("registry NBT unsupported"));
            }
        }
        Ok(Self { registry, entries })
    }
}
impl Packet for RegistryData {
    const ID: i32 = 7;
    const STATE: State = State::Configuration;
    const BOUND: Bound = Bound::Clientbound;
}
macro_rules! empty {
    ($name:ident, $bound:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name;
        impl Encode for $name {
            fn encode(&self, _: &mut Vec<u8>) {}
        }
        impl Decode for $name {
            fn decode(_: &mut &[u8]) -> Result<Self, DecodeError> {
                Ok(Self)
            }
        }
        impl Packet for $name {
            const ID: i32 = 3;
            const STATE: State = State::Configuration;
            const BOUND: Bound = Bound::$bound;
        }
    };
}
empty!(FinishConfiguration, Clientbound);
empty!(AcknowledgeFinishConfiguration, Serverbound);
packet!(ClientInformation, 0, Serverbound, {locale:String,view_distance:i8,chat_mode:VarInt,chat_colors:bool,skin_parts:u8,main_hand:VarInt,text_filtering:bool,server_listing:bool,particle_status:VarInt});
packet!(ConfigurationDisconnect, 2, Clientbound, {reason:crate::types::PlainText});
