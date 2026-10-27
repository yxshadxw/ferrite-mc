use super::login::Property;
use crate::{Bound, Decode, DecodeError, Encode, Packet, State, VarInt};
use uuid::Uuid;
macro_rules! fields { ($name:ident {$($f:ident:$t:ty),* $(,)?})=>{ #[derive(Debug,Clone,PartialEq)] pub struct $name {$(pub $f:$t),*} impl Encode for $name {fn encode(&self,out:&mut Vec<u8>){$(self.$f.encode(out);)*}} impl Decode for $name {fn decode(buf:&mut &[u8])->Result<Self,DecodeError>{Ok(Self{$($f:<$t>::decode(buf)?),*})}} }; }
macro_rules! packet {($name:ident,$id:expr,$bound:ident,{$($f:ident:$t:ty),* $(,)?})=>{ fields!($name {$($f:$t),*}); impl Packet for $name {const ID:i32=$id;const STATE:State=State::Play;const BOUND:Bound=Bound::$bound;} };}
fields!(DeathLocation {
    dimension: String,
    position: i64
});
packet!(PlayLogin,50,Clientbound,{entity_id:i32,hardcore:bool,dimensions:Vec<String>,max_players:VarInt,view_distance:VarInt,simulation_distance:VarInt,reduced_debug:bool,respawn_screen:bool,limited_crafting:bool,dimension_type:VarInt,dimension:String,seed:i64,game_mode:VarInt,previous_game_mode:VarInt,debug:bool,flat:bool,death_location:Option<DeathLocation>,portal_cooldown:VarInt,sea_level:VarInt,online_mode:bool,secure_chat:bool});
packet!(PositionSync,73,Clientbound,{teleport_id:VarInt,x:f64,y:f64,z:f64,velocity_x:f64,velocity_y:f64,velocity_z:f64,yaw:f32,pitch:f32,flags:i32});
packet!(GameEvent,39,Clientbound,{event:u8,value:f32});
packet!(ChunkBatchFinished,11,Clientbound,{size:VarInt});
packet!(CenterChunk,96,Clientbound,{x:VarInt,z:VarInt});
packet!(DefaultSpawn,99,Clientbound,{dimension:String,position:i64,yaw:f32,pitch:f32});
packet!(PlayKeepAlive,45,Clientbound,{id:i64});
packet!(ClientKeepAlive,28,Serverbound,{id:i64});
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkBatchStart;
impl Encode for ChunkBatchStart {
    fn encode(&self, _: &mut Vec<u8>) {}
}
impl Decode for ChunkBatchStart {
    fn decode(_: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self)
    }
}
impl Packet for ChunkBatchStart {
    const ID: i32 = 12;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Clientbound;
}
fields!(Heightmap {kind:VarInt,data:Vec<i64>});
#[derive(Debug, Clone, PartialEq)]
pub struct ChunkData {
    pub x: i32,
    pub z: i32,
    pub heightmaps: Vec<Heightmap>,
    pub data: Vec<u8>,
    pub sky_mask: Vec<i64>,
    pub block_mask: Vec<i64>,
    pub empty_sky_mask: Vec<i64>,
    pub empty_block_mask: Vec<i64>,
    pub sky_arrays: Vec<Vec<u8>>,
    pub block_arrays: Vec<Vec<u8>>,
}
impl Encode for ChunkData {
    fn encode(&self, out: &mut Vec<u8>) {
        self.x.encode(out);
        self.z.encode(out);
        self.heightmaps.encode(out);
        self.data.encode(out);
        VarInt(0).encode(out);
        self.sky_mask.encode(out);
        self.block_mask.encode(out);
        self.empty_sky_mask.encode(out);
        self.empty_block_mask.encode(out);
        self.sky_arrays.encode(out);
        self.block_arrays.encode(out);
    }
}
impl Decode for ChunkData {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let x = i32::decode(buf)?;
        let z = i32::decode(buf)?;
        let heightmaps = Vec::<Heightmap>::decode(buf)?;
        let data = Vec::<u8>::decode(buf)?;
        if VarInt::decode(buf)?.0 != 0 {
            return Err(DecodeError::Invalid("block entities unsupported"));
        }
        Ok(Self {
            x,
            z,
            heightmaps,
            data,
            sky_mask: Vec::decode(buf)?,
            block_mask: Vec::decode(buf)?,
            empty_sky_mask: Vec::decode(buf)?,
            empty_block_mask: Vec::decode(buf)?,
            sky_arrays: Vec::decode(buf)?,
            block_arrays: Vec::decode(buf)?,
        })
    }
}
impl Packet for ChunkData {
    const ID: i32 = 46;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Clientbound;
}
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfo {
    pub uuid: Uuid,
    pub name: String,
    pub properties: Vec<Property>,
}
impl Encode for PlayerInfo {
    fn encode(&self, out: &mut Vec<u8>) {
        1u8.encode(out);
        VarInt(1).encode(out);
        self.uuid.encode(out);
        self.name.encode(out);
        self.properties.encode(out);
    }
}
impl Decode for PlayerInfo {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        if u8::decode(buf)? != 1 || VarInt::decode(buf)?.0 != 1 {
            return Err(DecodeError::Invalid("player info actions"));
        }
        Ok(Self {
            uuid: Uuid::decode(buf)?,
            name: String::decode(buf)?,
            properties: Vec::decode(buf)?,
        })
    }
}
impl Packet for PlayerInfo {
    const ID: i32 = 71;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Clientbound;
}
packet!(ConfirmTeleport,0,Serverbound,{id:VarInt,x:f64,y:f64,z:f64,yaw:f32,pitch:f32});
packet!(MovePosition,30,Serverbound,{x:f64,y:f64,z:f64,flags:u8});
packet!(MovePositionRotation,31,Serverbound,{x:f64,y:f64,z:f64,yaw:f32,pitch:f32,flags:u8});
packet!(MoveRotation,32,Serverbound,{yaw:f32,pitch:f32,flags:u8});
packet!(MoveFlags,33,Serverbound,{flags:u8});
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerLoaded;
impl Encode for PlayerLoaded {
    fn encode(&self, _: &mut Vec<u8>) {}
}
impl Decode for PlayerLoaded {
    fn decode(_: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self)
    }
}
impl Packet for PlayerLoaded {
    const ID: i32 = 44;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Serverbound;
}
packet!(PlayDisconnect,32,Clientbound,{reason:crate::types::PlainText});
