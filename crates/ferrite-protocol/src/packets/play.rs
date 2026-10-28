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
/// Light masks are `java.util.BitSet`s. In 26.3 the client reads them as a VarInt-prefixed *byte* array
/// (`BitSet.toByteArray()`: little-endian, no trailing zero bytes), not as the long array older versions
/// and the wiki describe. Verified against the real client codec with `tools/wirecheck`.
fn encode_bitset(longs: &[i64], out: &mut Vec<u8>) {
    let mut bytes: Vec<u8> = longs.iter().flat_map(|l| l.to_le_bytes()).collect();
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    VarInt(bytes.len() as i32).encode(out);
    out.extend_from_slice(&bytes);
}
fn decode_bitset(buf: &mut &[u8]) -> Result<Vec<i64>, DecodeError> {
    let mut bytes = Vec::<u8>::decode(buf)?;
    bytes.resize(bytes.len().div_ceil(8) * 8, 0);
    Ok(bytes
        .chunks_exact(8)
        .map(|c| i64::from_le_bytes(c.try_into().expect("chunk of 8")))
        .collect())
}
impl Encode for ChunkData {
    fn encode(&self, out: &mut Vec<u8>) {
        self.x.encode(out);
        self.z.encode(out);
        self.heightmaps.encode(out);
        self.data.encode(out);
        VarInt(0).encode(out);
        encode_bitset(&self.sky_mask, out);
        encode_bitset(&self.block_mask, out);
        encode_bitset(&self.empty_sky_mask, out);
        encode_bitset(&self.empty_block_mask, out);
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
            sky_mask: decode_bitset(buf)?,
            block_mask: decode_bitset(buf)?,
            empty_sky_mask: decode_bitset(buf)?,
            empty_block_mask: decode_bitset(buf)?,
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
    pub game_mode: VarInt,
}
impl Encode for PlayerInfo {
    fn encode(&self, out: &mut Vec<u8>) {
        13u8.encode(out);
        VarInt(1).encode(out);
        self.uuid.encode(out);
        self.name.encode(out);
        self.properties.encode(out);
        self.game_mode.encode(out);
        true.encode(out);
    }
}
impl Decode for PlayerInfo {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        if u8::decode(buf)? != 13 || VarInt::decode(buf)?.0 != 1 {
            return Err(DecodeError::Invalid("player info actions"));
        }
        let uuid = Uuid::decode(buf)?;
        let name = String::decode(buf)?;
        let properties = Vec::decode(buf)?;
        let game_mode = VarInt::decode(buf)?;
        if !bool::decode(buf)? {
            return Err(DecodeError::Invalid("unlisted player"));
        }
        Ok(Self {
            uuid,
            name,
            properties,
            game_mode,
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
packet!(ForgetChunk,38,Clientbound,{z:i32,x:i32});
packet!(EntityMove,54,Clientbound,{id:VarInt,properties:VarInt,dx:i16,dy:i16,dz:i16});
packet!(EntityMoveRotation,55,Clientbound,{id:VarInt,properties:VarInt,dx:i16,dy:i16,dz:i16,yaw:u8,pitch:u8});
packet!(EntityRotation,57,Clientbound,{id:VarInt,on_ground:bool,yaw:u8,pitch:u8});
packet!(EntityHeadRotation,85,Clientbound,{id:VarInt,yaw:u8});
packet!(EntityTeleport,35,Clientbound,{id:VarInt,path_type:VarInt,x:f64,y:f64,z:f64,yaw:f32,pitch:f32,on_ground:bool});
packet!(RemoveEntities,78,Clientbound,{ids:Vec<VarInt>});
packet!(PlayerInfoRemove,70,Clientbound,{uuids:Vec<Uuid>});
packet!(SystemChat,124,Clientbound,{content:crate::types::PlainText,overlay:bool});
packet!(ChunkBatchReceived,11,Serverbound,{chunks_per_tick:f32});
#[derive(Debug, Clone, PartialEq)]
pub struct AddEntity {
    pub id: VarInt,
    pub uuid: Uuid,
    pub entity_type: VarInt,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub pitch: u8,
    pub yaw: u8,
    pub head_yaw: u8,
    pub data: VarInt,
}
impl Encode for AddEntity {
    fn encode(&self, out: &mut Vec<u8>) {
        self.id.encode(out);
        self.uuid.encode(out);
        self.entity_type.encode(out);
        self.x.encode(out);
        self.y.encode(out);
        self.z.encode(out);
        0u8.encode(out);
        self.pitch.encode(out);
        self.yaw.encode(out);
        self.head_yaw.encode(out);
        self.data.encode(out);
    }
}
impl Decode for AddEntity {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let id = VarInt::decode(buf)?;
        let uuid = Uuid::decode(buf)?;
        let entity_type = VarInt::decode(buf)?;
        let x = f64::decode(buf)?;
        let y = f64::decode(buf)?;
        let z = f64::decode(buf)?;
        if u8::decode(buf)? != 0 {
            return Err(DecodeError::Invalid("nonzero velocity unsupported"));
        }
        Ok(Self {
            id,
            uuid,
            entity_type,
            x,
            y,
            z,
            pitch: u8::decode(buf)?,
            yaw: u8::decode(buf)?,
            head_yaw: u8::decode(buf)?,
            data: VarInt::decode(buf)?,
        })
    }
}
impl Packet for AddEntity {
    const ID: i32 = 1;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Clientbound;
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerGameMode {
    pub uuid: Uuid,
    pub game_mode: VarInt,
}
impl Encode for PlayerGameMode {
    fn encode(&self, out: &mut Vec<u8>) {
        4u8.encode(out);
        VarInt(1).encode(out);
        self.uuid.encode(out);
        self.game_mode.encode(out);
    }
}
impl Decode for PlayerGameMode {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        if u8::decode(buf)? != 4 || VarInt::decode(buf)?.0 != 1 {
            return Err(DecodeError::Invalid("game mode update"));
        }
        Ok(Self {
            uuid: Uuid::decode(buf)?,
            game_mode: VarInt::decode(buf)?,
        })
    }
}
impl Packet for PlayerGameMode {
    const ID: i32 = 71;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Clientbound;
}
packet!(PlayerAction,41,Serverbound,{status:VarInt,location:i64,face:i8,sequence:VarInt});
packet!(UseItemOn,66,Serverbound,{hand:VarInt,location:i64,face:VarInt,cursor_x:f32,cursor_y:f32,cursor_z:f32,inside:bool,world_border_hit:bool,sequence:VarInt});
packet!(BlockUpdate,8,Clientbound,{location:i64,block_state:VarInt});
packet!(AcknowledgeBlockChange,4,Clientbound,{sequence:VarInt});
packet!(SetHeldItem,54,Serverbound,{slot:i16});
packet!(CreativeSlot,57,Serverbound,{slot:i16,item:ItemStack});
packet!(ContainerContent,18,Clientbound,{window:VarInt,state:VarInt,slots:Vec<ItemStack>,carried:ItemStack});
packet!(ContainerSlot,20,Clientbound,{window:VarInt,state:VarInt,slot:i16,item:ItemStack});

/// Minimal item stack; component patches are deliberately rejected until supported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ItemStack {
    pub count: i32,
    pub item_id: i32,
}
impl Encode for ItemStack {
    fn encode(&self, out: &mut Vec<u8>) {
        VarInt(self.count).encode(out);
        if self.count > 0 {
            VarInt(self.item_id).encode(out);
            VarInt(0).encode(out);
            VarInt(0).encode(out);
        }
    }
}
impl Decode for ItemStack {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let count = VarInt::decode(buf)?.0;
        if count == 0 {
            return Ok(Self::default());
        }
        if !(1..=99).contains(&count) {
            return Err(DecodeError::Invalid("invalid item count"));
        }
        let item_id = VarInt::decode(buf)?.0;
        if item_id < 0 {
            return Err(DecodeError::Invalid("invalid item id"));
        }
        if VarInt::decode(buf)?.0 != 0 || VarInt::decode(buf)?.0 != 0 {
            return Err(DecodeError::Invalid("item components unsupported"));
        }
        Ok(Self { count, item_id })
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    fn roundtrip<T: Encode + Decode + PartialEq + std::fmt::Debug>(value: T) {
        let mut bytes = Vec::new();
        value.encode(&mut bytes);
        let mut data = bytes.as_slice();
        assert_eq!(T::decode(&mut data).unwrap(), value);
        assert!(data.is_empty());
    }
    #[test]
    fn interaction_packets_roundtrip() {
        roundtrip(PlayerGameMode {
            uuid: Uuid::nil(),
            game_mode: VarInt(1),
        });
        roundtrip(PlayerAction {
            status: VarInt(0),
            location: 15,
            face: 1,
            sequence: VarInt(42),
        });
        roundtrip(UseItemOn {
            hand: VarInt(0),
            location: 15,
            face: VarInt(1),
            cursor_x: 0.5,
            cursor_y: 1.0,
            cursor_z: 0.5,
            inside: false,
            world_border_hit: false,
            sequence: VarInt(3),
        });
        roundtrip(BlockUpdate {
            location: 15,
            block_state: VarInt(9),
        });
        roundtrip(AcknowledgeBlockChange {
            sequence: VarInt(3),
        });
        roundtrip(SetHeldItem { slot: 2 });
        roundtrip(CreativeSlot {
            slot: 36,
            item: ItemStack {
                count: 3,
                item_id: 7,
            },
        });
        roundtrip(ContainerContent {
            window: VarInt(0),
            state: VarInt(0),
            slots: vec![
                ItemStack::default(),
                ItemStack {
                    count: 1,
                    item_id: 2,
                },
            ],
            carried: ItemStack::default(),
        });
        roundtrip(ContainerSlot {
            window: VarInt(0),
            state: VarInt(3),
            slot: 36,
            item: ItemStack::default(),
        });
    }
}
