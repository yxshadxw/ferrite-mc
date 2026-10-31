use super::login::Property;
use crate::{Bound, Decode, DecodeError, Encode, Packet, State, VarInt, VarLong, types::LpVec3};
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
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| i64::from_le_bytes(*c))
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
/// `ADD_PLAYER | INITIALIZE_CHAT | UPDATE_GAME_MODE | UPDATE_LISTED`: the actions every player
/// needs for the tab list and for validating chat signatures.
const PLAYER_INFO_ACTIONS: u8 = 1 | 2 | 4 | 8;
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfo {
    pub uuid: Uuid,
    pub name: String,
    pub properties: Vec<Property>,
    pub chat_session: Option<RemoteChatSessionData>,
    pub game_mode: VarInt,
}
impl Encode for PlayerInfo {
    fn encode(&self, out: &mut Vec<u8>) {
        PLAYER_INFO_ACTIONS.encode(out);
        VarInt(1).encode(out);
        self.uuid.encode(out);
        self.name.encode(out);
        self.properties.encode(out);
        self.chat_session.encode(out);
        self.game_mode.encode(out);
        true.encode(out);
    }
}
impl Decode for PlayerInfo {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        if u8::decode(buf)? != PLAYER_INFO_ACTIONS || VarInt::decode(buf)?.0 != 1 {
            return Err(DecodeError::Invalid("player info actions"));
        }
        let uuid = Uuid::decode(buf)?;
        let name = String::decode(buf)?;
        let properties = Vec::decode(buf)?;
        let chat_session = Option::decode(buf)?;
        let game_mode = VarInt::decode(buf)?;
        if !bool::decode(buf)? {
            return Err(DecodeError::Invalid("unlisted player"));
        }
        Ok(Self {
            uuid,
            name,
            properties,
            chat_session,
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
    pub velocity: LpVec3,
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
        self.velocity.encode(out);
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
        Ok(Self {
            id,
            uuid,
            entity_type,
            x,
            y,
            z,
            velocity: LpVec3::decode(buf)?,
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
packet!(SetEntityMotion,103,Clientbound,{id:VarInt,velocity:LpVec3});
packet!(TakeItemEntity,127,Clientbound,{item:VarInt,collector:VarInt,amount:VarInt});
// `teleport_entity`: an explicit teleport (`PositionMoveRotation`, relative flags as an int bit
// set, on-ground). Movement updates use `entity_position_sync` (`EntityTeleport`).
packet!(TeleportEntity,128,Clientbound,{id:VarInt,x:f64,y:f64,z:f64,velocity_x:f64,velocity_y:f64,velocity_z:f64,yaw:f32,pitch:f32,relatives:i32,on_ground:bool});

/// One synched entity data value, tagged with its `EntityDataSerializers` id on the wire.
#[derive(Debug, Clone, PartialEq)]
pub enum MetaValue {
    Byte(i8),
    Int(i32),
    Long(i64),
    Float(f32),
    String(String),
    ItemStack(ItemStack),
    Boolean(bool),
    Pose(i32),
}
impl MetaValue {
    /// Serializer id in `EntityDataSerializers` registration order (26.3).
    pub fn serializer(&self) -> i32 {
        match self {
            Self::Byte(_) => 0,
            Self::Int(_) => 1,
            Self::Long(_) => 2,
            Self::Float(_) => 3,
            Self::String(_) => 4,
            Self::ItemStack(_) => 7,
            Self::Boolean(_) => 8,
            Self::Pose(_) => 20,
        }
    }
}
/// A data value at its accessor index.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaEntry {
    pub index: u8,
    pub value: MetaValue,
}
impl Encode for MetaEntry {
    fn encode(&self, out: &mut Vec<u8>) {
        self.index.encode(out);
        VarInt(self.value.serializer()).encode(out);
        match &self.value {
            MetaValue::Byte(v) => v.encode(out),
            MetaValue::Int(v) | MetaValue::Pose(v) => VarInt(*v).encode(out),
            MetaValue::Long(v) => VarLong(*v).encode(out),
            MetaValue::Float(v) => v.encode(out),
            MetaValue::String(v) => v.encode(out),
            MetaValue::ItemStack(v) => v.encode(out),
            MetaValue::Boolean(v) => v.encode(out),
        }
    }
}
impl MetaEntry {
    fn decode_at(index: u8, buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let value = match VarInt::decode(buf)?.0 {
            0 => MetaValue::Byte(i8::decode(buf)?),
            1 => MetaValue::Int(VarInt::decode(buf)?.0),
            2 => MetaValue::Long(VarLong::decode(buf)?.0),
            3 => MetaValue::Float(f32::decode(buf)?),
            4 => MetaValue::String(String::decode(buf)?),
            7 => MetaValue::ItemStack(ItemStack::decode(buf)?),
            8 => MetaValue::Boolean(bool::decode(buf)?),
            20 => MetaValue::Pose(VarInt::decode(buf)?.0),
            _ => return Err(DecodeError::Invalid("unsupported entity data serializer")),
        };
        Ok(Self { index, value })
    }
}
/// `set_entity_data`: changed data values, terminated by index 255.
#[derive(Debug, Clone, PartialEq)]
pub struct SetEntityData {
    pub id: VarInt,
    pub values: Vec<MetaEntry>,
}
impl Encode for SetEntityData {
    fn encode(&self, out: &mut Vec<u8>) {
        self.id.encode(out);
        for value in &self.values {
            value.encode(out);
        }
        out.push(0xFF);
    }
}
impl Decode for SetEntityData {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let id = VarInt::decode(buf)?;
        let mut values = Vec::new();
        loop {
            match u8::decode(buf)? {
                0xFF => return Ok(Self { id, values }),
                index => values.push(MetaEntry::decode_at(index, buf)?),
            }
        }
    }
}
impl Packet for SetEntityData {
    const ID: i32 = 101;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Clientbound;
}
packet!(PlayerAction,41,Serverbound,{status:VarInt,location:i64,face:i8,sequence:VarInt});
packet!(UseItemOn,66,Serverbound,{hand:VarInt,location:i64,face:VarInt,cursor_x:f32,cursor_y:f32,cursor_z:f32,inside:bool,world_border_hit:bool,sequence:VarInt});
packet!(BlockUpdate,8,Clientbound,{location:i64,block_state:VarInt});
packet!(AcknowledgeBlockChange,4,Clientbound,{sequence:VarInt});
packet!(SetHeldItem,54,Serverbound,{slot:i16});

/// Packets carrying item stacks are generic over the stack type: this crate cannot depend on
/// `ferrite-item`, whose full `ItemStack` (and `HashedSlot`) implement `Encode`/`Decode` behind
/// its `protocol` feature. The default is the minimal [`ItemStack`].
macro_rules! item_packet {($name:ident<$g:ident $(= $d:ty)?>,$id:expr,$bound:ident,{$($f:ident:$t:ty),* $(,)?})=>{
    #[derive(Debug,Clone,PartialEq)] pub struct $name<$g $(= $d)?> {$(pub $f:$t),*}
    impl<$g: Encode> Encode for $name<$g> {fn encode(&self,out:&mut Vec<u8>){$(self.$f.encode(out);)*}}
    impl<$g: Decode> Decode for $name<$g> {fn decode(buf:&mut &[u8])->Result<Self,DecodeError>{Ok(Self{$($f:<$t>::decode(buf)?),*})}}
    impl<$g: Encode + Decode> Packet for $name<$g> {const ID:i32=$id;const STATE:State=State::Play;const BOUND:Bound=Bound::$bound;}
};}
item_packet!(CreativeSlot<I = ItemStack>,57,Serverbound,{slot:i16,item:I});
item_packet!(ContainerContent<I = ItemStack>,18,Clientbound,{window:VarInt,state:VarInt,slots:Vec<I>,carried:I});
item_packet!(ContainerSlot<I = ItemStack>,20,Clientbound,{window:VarInt,state:VarInt,slot:i16,item:I});
item_packet!(SetCursorItem<I = ItemStack>,98,Clientbound,{item:I});
item_packet!(SetPlayerInventory<I = ItemStack>,110,Clientbound,{slot:VarInt,item:I});
packet!(ServerboundContainerClose,19,Serverbound,{window:VarInt});
packet!(ClientboundContainerClose,17,Clientbound,{window:VarInt});
packet!(SetHeldSlot,107,Clientbound,{slot:VarInt});
packet!(OpenScreen,60,Clientbound,{window:VarInt,menu:VarInt,title:crate::types::Translatable});

/// Vanilla `ServerboundContainerClickPacket.MAX_SLOT_COUNT`.
pub const CONTAINER_CLICK_MAX_SLOTS: usize = 128;

/// `container_click`. `H` is the hashed stack (`ferrite_item::HashedSlot`); `changed` holds at
/// most [`CONTAINER_CLICK_MAX_SLOTS`] `(slot, predicted stack)` pairs, `None` being empty.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainerClick<H> {
    pub window: VarInt,
    pub state: VarInt,
    pub slot: i16,
    pub button: i8,
    pub input: VarInt,
    pub changed: Vec<(i16, Option<H>)>,
    pub carried: Option<H>,
}
impl<H: Encode> Encode for ContainerClick<H> {
    fn encode(&self, out: &mut Vec<u8>) {
        self.window.encode(out);
        self.state.encode(out);
        self.slot.encode(out);
        self.button.encode(out);
        self.input.encode(out);
        VarInt(self.changed.len() as i32).encode(out);
        for (slot, item) in &self.changed {
            slot.encode(out);
            item.encode(out);
        }
        self.carried.encode(out);
    }
}
impl<H: Decode> Decode for ContainerClick<H> {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let window = VarInt::decode(buf)?;
        let state = VarInt::decode(buf)?;
        let slot = i16::decode(buf)?;
        let button = i8::decode(buf)?;
        let input = VarInt::decode(buf)?;
        let len = usize::try_from(VarInt::decode(buf)?.0)
            .ok()
            .filter(|&len| len <= CONTAINER_CLICK_MAX_SLOTS)
            .ok_or(DecodeError::Invalid("too many changed slots"))?;
        let mut changed = Vec::with_capacity(len);
        for _ in 0..len {
            changed.push((i16::decode(buf)?, Option::<H>::decode(buf)?));
        }
        let carried = Option::<H>::decode(buf)?;
        Ok(Self {
            window,
            state,
            slot,
            button,
            input,
            changed,
            carried,
        })
    }
}
impl<H: Encode + Decode> Packet for ContainerClick<H> {
    const ID: i32 = 18;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Serverbound;
}

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

/// A 256-byte `SHA256withRSA` chat signature (`MessageSignature` in vanilla). ED25519 was never
/// used for Java chat: profile keys are 2048-bit RSA keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MessageSignature(pub [u8; 256]);
impl Encode for MessageSignature {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0);
    }
}
impl Decode for MessageSignature {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let bytes = crate::codec::take(buf, 256)?;
        Ok(Self(bytes.try_into().expect("exactly 256 bytes")))
    }
}

/// `LastSeenMessages.Update`: offset into the acknowledged queue, a 20-bit bitset (three bytes,
/// little-endian) of acknowledged entries, and the checksum (zero means unchecked).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastSeenUpdate {
    pub offset: VarInt,
    pub acknowledged: [u8; 3],
    pub checksum: i8,
}
impl LastSeenUpdate {
    pub fn is_acknowledged(&self, index: usize) -> bool {
        index < 20 && self.acknowledged[index / 8] & (1 << (index % 8)) != 0
    }
    pub fn new(offset: i32, acknowledged: &[usize], checksum: i8) -> Self {
        let mut bits = [0u8; 3];
        for &index in acknowledged {
            bits[index / 8] |= 1 << (index % 8);
        }
        Self {
            offset: VarInt(offset),
            acknowledged: bits,
            checksum,
        }
    }
}
impl Encode for LastSeenUpdate {
    fn encode(&self, out: &mut Vec<u8>) {
        self.offset.encode(out);
        out.extend_from_slice(&self.acknowledged);
        self.checksum.encode(out);
    }
}
impl Decode for LastSeenUpdate {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            offset: VarInt::decode(buf)?,
            acknowledged: crate::codec::take(buf, 3)?
                .try_into()
                .expect("exactly 3 bytes"),
            checksum: i8::decode(buf)?,
        })
    }
}

/// `ProfilePublicKey.Data`: expiry (epoch millis), X.509 RSA key, and Mojang's signature over both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileKeyData {
    pub expires_at: i64,
    pub key: Vec<u8>,
    pub key_signature: Vec<u8>,
}
impl Encode for ProfileKeyData {
    fn encode(&self, out: &mut Vec<u8>) {
        self.expires_at.encode(out);
        self.key.encode(out);
        self.key_signature.encode(out);
    }
}
impl Decode for ProfileKeyData {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let expires_at = i64::decode(buf)?;
        let key = decode_byte_array(buf, 512)?;
        let key_signature = decode_byte_array(buf, 4096)?;
        Ok(Self {
            expires_at,
            key,
            key_signature,
        })
    }
}
fn decode_byte_array(buf: &mut &[u8], max: usize) -> Result<Vec<u8>, DecodeError> {
    let len = VarInt::decode(buf)?.0;
    let len = usize::try_from(len).map_err(|_| DecodeError::Invalid("negative array length"))?;
    if len > max {
        return Err(DecodeError::Invalid("byte array too long"));
    }
    Ok(crate::codec::take(buf, len)?.to_vec())
}

/// `RemoteChatSession.Data`: the session UUID plus the profile public key. Sent by the client in
/// `chat_session_update` and echoed to every client in `player_info` so it can verify signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteChatSessionData {
    pub session_id: Uuid,
    pub profile_key: ProfileKeyData,
}
impl Encode for RemoteChatSessionData {
    fn encode(&self, out: &mut Vec<u8>) {
        self.session_id.encode(out);
        self.profile_key.encode(out);
    }
}
impl Decode for RemoteChatSessionData {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            session_id: Uuid::decode(buf)?,
            profile_key: ProfileKeyData::decode(buf)?,
        })
    }
}

/// `ClientboundPlayerInfoUpdatePacket` carrying only `INITIALIZE_CHAT`, broadcast when a player's
/// chat session changes after joining.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfoChatSession {
    pub uuid: Uuid,
    pub chat_session: Option<RemoteChatSessionData>,
}
impl Encode for PlayerInfoChatSession {
    fn encode(&self, out: &mut Vec<u8>) {
        2u8.encode(out);
        VarInt(1).encode(out);
        self.uuid.encode(out);
        self.chat_session.encode(out);
    }
}
impl Decode for PlayerInfoChatSession {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        if u8::decode(buf)? != 2 || VarInt::decode(buf)?.0 != 1 {
            return Err(DecodeError::Invalid("chat session update actions"));
        }
        Ok(Self {
            uuid: Uuid::decode(buf)?,
            chat_session: Option::decode(buf)?,
        })
    }
}
impl Packet for PlayerInfoChatSession {
    const ID: i32 = 71;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Clientbound;
}

/// Argument signature used by signed chat commands (`ArgumentSignatures.Entry`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentSignature {
    pub name: String,
    pub signature: MessageSignature,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundChat {
    pub message: String,
    /// Epoch milliseconds.
    pub timestamp: i64,
    pub salt: i64,
    pub signature: Option<MessageSignature>,
    pub last_seen: LastSeenUpdate,
}
impl Encode for ServerboundChat {
    fn encode(&self, out: &mut Vec<u8>) {
        self.message.encode(out);
        self.timestamp.encode(out);
        self.salt.encode(out);
        self.signature.encode(out);
        self.last_seen.encode(out);
    }
}
impl Decode for ServerboundChat {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            message: crate::codec::decode_string(buf, 256)?,
            timestamp: i64::decode(buf)?,
            salt: i64::decode(buf)?,
            signature: Option::decode(buf)?,
            last_seen: LastSeenUpdate::decode(buf)?,
        })
    }
}
impl Packet for ServerboundChat {
    const ID: i32 = 9;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Serverbound;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundChatAck {
    pub offset: VarInt,
}
impl Encode for ServerboundChatAck {
    fn encode(&self, out: &mut Vec<u8>) {
        self.offset.encode(out);
    }
}
impl Decode for ServerboundChatAck {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            offset: VarInt::decode(buf)?,
        })
    }
}
impl Packet for ServerboundChatAck {
    const ID: i32 = 6;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Serverbound;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundChatCommand {
    pub command: String,
}
impl Encode for ServerboundChatCommand {
    fn encode(&self, out: &mut Vec<u8>) {
        self.command.encode(out);
    }
}
impl Decode for ServerboundChatCommand {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            command: String::decode(buf)?,
        })
    }
}
impl Packet for ServerboundChatCommand {
    const ID: i32 = 7;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Serverbound;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundChatCommandSigned {
    pub command: String,
    pub timestamp: i64,
    pub salt: i64,
    pub argument_signatures: Vec<ArgumentSignature>,
    pub last_seen: LastSeenUpdate,
}
impl Encode for ServerboundChatCommandSigned {
    fn encode(&self, out: &mut Vec<u8>) {
        self.command.encode(out);
        self.timestamp.encode(out);
        self.salt.encode(out);
        VarInt(self.argument_signatures.len() as i32).encode(out);
        for entry in &self.argument_signatures {
            entry.name.encode(out);
            entry.signature.encode(out);
        }
        self.last_seen.encode(out);
    }
}
impl Decode for ServerboundChatCommandSigned {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let command = String::decode(buf)?;
        let timestamp = i64::decode(buf)?;
        let salt = i64::decode(buf)?;
        let len = VarInt::decode(buf)?.0;
        let len =
            usize::try_from(len).map_err(|_| DecodeError::Invalid("negative array length"))?;
        if len > 8 {
            return Err(DecodeError::Invalid("too many argument signatures"));
        }
        let mut argument_signatures = Vec::with_capacity(len);
        for _ in 0..len {
            argument_signatures.push(ArgumentSignature {
                name: crate::codec::decode_string(buf, 16)?,
                signature: MessageSignature::decode(buf)?,
            });
        }
        Ok(Self {
            command,
            timestamp,
            salt,
            argument_signatures,
            last_seen: LastSeenUpdate::decode(buf)?,
        })
    }
}
impl Packet for ServerboundChatCommandSigned {
    const ID: i32 = 8;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Serverbound;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundChatSessionUpdate {
    pub chat_session: RemoteChatSessionData,
}
impl Encode for ServerboundChatSessionUpdate {
    fn encode(&self, out: &mut Vec<u8>) {
        self.chat_session.encode(out);
    }
}
impl Decode for ServerboundChatSessionUpdate {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            chat_session: RemoteChatSessionData::decode(buf)?,
        })
    }
}
impl Packet for ServerboundChatSessionUpdate {
    const ID: i32 = 10;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Serverbound;
}

/// The chat-type registry holder (`VarInt` id + 1, 0 meaning an inline definition), the sender
/// name shown by the chat decoration, and an optional recipient name for private messages.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatTypeBound {
    pub chat_type: VarInt,
    pub name: crate::types::PlainText,
    pub target_name: Option<crate::types::PlainText>,
}
impl Encode for ChatTypeBound {
    fn encode(&self, out: &mut Vec<u8>) {
        VarInt(self.chat_type.0 + 1).encode(out);
        self.name.encode(out);
        self.target_name.encode(out);
    }
}
impl Decode for ChatTypeBound {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let id = VarInt::decode(buf)?.0;
        if id == 0 {
            return Err(DecodeError::Invalid("inline chat types unsupported"));
        }
        Ok(Self {
            chat_type: VarInt(id - 1),
            name: crate::types::PlainText::decode(buf)?,
            target_name: Option::decode(buf)?,
        })
    }
}

/// `SignedMessageBody.Packed`: content, epoch-millisecond timestamp, salt, and the signature
/// chain's last-seen entries (encoded as full signatures).
#[derive(Debug, Clone, PartialEq)]
pub struct SignedMessageBody {
    pub content: String,
    pub timestamp: i64,
    pub salt: i64,
    pub last_seen: Vec<MessageSignature>,
}
impl Encode for SignedMessageBody {
    fn encode(&self, out: &mut Vec<u8>) {
        self.content.encode(out);
        self.timestamp.encode(out);
        self.salt.encode(out);
        VarInt(self.last_seen.len() as i32).encode(out);
        for signature in &self.last_seen {
            // `MessageSignature.Packed` id -1: the full 256-byte signature follows.
            VarInt(0).encode(out);
            signature.encode(out);
        }
    }
}
impl Decode for SignedMessageBody {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let content = crate::codec::decode_string(buf, 256)?;
        let timestamp = i64::decode(buf)?;
        let salt = i64::decode(buf)?;
        let len = VarInt::decode(buf)?.0;
        let len =
            usize::try_from(len).map_err(|_| DecodeError::Invalid("negative array length"))?;
        if len > 20 {
            return Err(DecodeError::Invalid("too many last-seen signatures"));
        }
        let mut last_seen = Vec::with_capacity(len);
        for _ in 0..len {
            if VarInt::decode(buf)?.0 != 0 {
                return Err(DecodeError::Invalid("packed signature cache unsupported"));
            }
            last_seen.push(MessageSignature::decode(buf)?);
        }
        Ok(Self {
            content,
            timestamp,
            salt,
            last_seen,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClientboundPlayerChat {
    /// Per-recipient chat index (the client's acknowledgement counter).
    pub global_index: VarInt,
    pub sender: Uuid,
    /// The sender's signature-chain index.
    pub index: VarInt,
    pub signature: Option<MessageSignature>,
    pub body: SignedMessageBody,
    pub unsigned_content: Option<crate::types::PlainText>,
    /// `FilterMask` type id; 0 is pass-through.
    pub filter_mask: VarInt,
    pub chat_type: ChatTypeBound,
}
impl Encode for ClientboundPlayerChat {
    fn encode(&self, out: &mut Vec<u8>) {
        self.global_index.encode(out);
        self.sender.encode(out);
        self.index.encode(out);
        self.signature.encode(out);
        self.body.encode(out);
        self.unsigned_content.encode(out);
        self.filter_mask.encode(out);
        self.chat_type.encode(out);
    }
}
impl Decode for ClientboundPlayerChat {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            global_index: VarInt::decode(buf)?,
            sender: Uuid::decode(buf)?,
            index: VarInt::decode(buf)?,
            signature: Option::decode(buf)?,
            body: SignedMessageBody::decode(buf)?,
            unsigned_content: Option::decode(buf)?,
            filter_mask: VarInt::decode(buf)?,
            chat_type: ChatTypeBound::decode(buf)?,
        })
    }
}
impl Packet for ClientboundPlayerChat {
    const ID: i32 = 66;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Clientbound;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClientboundDisguisedChat {
    pub message: crate::types::PlainText,
    pub chat_type: ChatTypeBound,
}
impl Encode for ClientboundDisguisedChat {
    fn encode(&self, out: &mut Vec<u8>) {
        self.message.encode(out);
        self.chat_type.encode(out);
    }
}
impl Decode for ClientboundDisguisedChat {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self {
            message: crate::types::PlainText::decode(buf)?,
            chat_type: ChatTypeBound::decode(buf)?,
        })
    }
}
impl Packet for ClientboundDisguisedChat {
    const ID: i32 = 33;
    const STATE: State = State::Play;
    const BOUND: Bound = Bound::Clientbound;
}

#[cfg(test)]
mod chat_tests {
    use super::*;
    fn roundtrip<T: Encode + Decode + PartialEq + std::fmt::Debug>(value: T) {
        let mut bytes = Vec::new();
        value.encode(&mut bytes);
        let mut data = bytes.as_slice();
        assert_eq!(T::decode(&mut data).unwrap(), value);
        assert!(data.is_empty());
    }
    fn signature(seed: u8) -> MessageSignature {
        MessageSignature([seed; 256])
    }
    fn profile_key() -> ProfileKeyData {
        ProfileKeyData {
            expires_at: 1_700_000_000_000,
            key: vec![1, 2, 3],
            key_signature: vec![4, 5],
        }
    }
    fn session() -> RemoteChatSessionData {
        RemoteChatSessionData {
            session_id: Uuid::from_u128(7),
            profile_key: profile_key(),
        }
    }
    fn chat_type() -> ChatTypeBound {
        ChatTypeBound {
            chat_type: VarInt(0),
            name: crate::types::PlainText("Steve".into()),
            target_name: None,
        }
    }
    #[test]
    fn chat_packets_roundtrip() {
        roundtrip(ServerboundChat {
            message: "hello".into(),
            timestamp: 1_700_000_000_123,
            salt: 42,
            signature: Some(signature(3)),
            last_seen: LastSeenUpdate::new(2, &[0, 4], 9),
        });
        roundtrip(ServerboundChatAck { offset: VarInt(3) });
        roundtrip(ServerboundChatCommand {
            command: "gamemode creative".into(),
        });
        roundtrip(ServerboundChatCommandSigned {
            command: "say hi".into(),
            timestamp: 1,
            salt: 2,
            argument_signatures: vec![ArgumentSignature {
                name: "message".into(),
                signature: signature(4),
            }],
            last_seen: LastSeenUpdate::new(0, &[], 1),
        });
        roundtrip(ServerboundChatSessionUpdate {
            chat_session: session(),
        });
        roundtrip(PlayerInfoChatSession {
            uuid: Uuid::from_u128(9),
            chat_session: Some(session()),
        });
        roundtrip(PlayerInfoChatSession {
            uuid: Uuid::from_u128(9),
            chat_session: None,
        });
        roundtrip(ClientboundPlayerChat {
            global_index: VarInt(5),
            sender: Uuid::from_u128(9),
            index: VarInt(2),
            signature: Some(signature(8)),
            body: SignedMessageBody {
                content: "hello".into(),
                timestamp: 1_700_000_000_123,
                salt: 42,
                last_seen: vec![signature(1), signature(2)],
            },
            unsigned_content: None,
            filter_mask: VarInt(0),
            chat_type: chat_type(),
        });
        roundtrip(ClientboundDisguisedChat {
            message: crate::types::PlainText("hi".into()),
            chat_type: chat_type(),
        });
    }
    #[test]
    fn chat_type_uses_holder_id_plus_one() {
        let mut bytes = Vec::new();
        chat_type().encode(&mut bytes);
        assert_eq!(bytes[0], 1);
    }
    #[test]
    fn last_seen_bitset_is_three_bytes() {
        let update = LastSeenUpdate::new(0, &[0, 7, 19], 0);
        assert!(update.is_acknowledged(0));
        assert!(update.is_acknowledged(7));
        assert!(update.is_acknowledged(19));
        assert!(!update.is_acknowledged(1));
        let mut bytes = Vec::new();
        LastSeenUpdate::new(0, &[], 0).encode(&mut bytes);
        assert_eq!(bytes, [0, 0, 0, 0, 0]);
    }
    #[test]
    fn rejects_oversized_profile_key() {
        let mut bytes = Vec::new();
        Uuid::nil().encode(&mut bytes);
        0i64.encode(&mut bytes);
        VarInt(513).encode(&mut bytes);
        bytes.extend(std::iter::repeat_n(0u8, 513));
        assert!(RemoteChatSessionData::decode(&mut bytes.as_slice()).is_err());
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
    fn container_packets_roundtrip() {
        let stone = ItemStack {
            count: 3,
            item_id: 1,
        };
        roundtrip(ContainerClick::<ItemStack> {
            window: VarInt(1),
            state: VarInt(7),
            slot: 36,
            button: 1,
            input: VarInt(5),
            changed: vec![(36, None), (9, Some(stone))],
            carried: Some(stone),
        });
        roundtrip(SetCursorItem { item: stone });
        roundtrip(SetPlayerInventory {
            slot: VarInt(40),
            item: stone,
        });
        roundtrip(ServerboundContainerClose { window: VarInt(3) });
        roundtrip(ClientboundContainerClose { window: VarInt(3) });
        roundtrip(SetHeldSlot { slot: VarInt(4) });
        roundtrip(OpenScreen {
            window: VarInt(1),
            menu: VarInt(12),
            title: crate::types::Translatable("container.crafting".into()),
        });
        // More than 128 changed slots is rejected like vanilla.
        let mut bytes = Vec::new();
        ContainerClick::<ItemStack> {
            window: VarInt(0),
            state: VarInt(0),
            slot: 0,
            button: 0,
            input: VarInt(0),
            changed: vec![(0, None); 129],
            carried: None,
        }
        .encode(&mut bytes);
        assert!(ContainerClick::<ItemStack>::decode(&mut bytes.as_slice()).is_err());
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
    #[test]
    fn entity_packets_roundtrip() {
        roundtrip(AddEntity {
            id: VarInt(5),
            uuid: Uuid::from_u128(7),
            entity_type: VarInt(71),
            x: 0.5,
            y: 64.0,
            z: -3.25,
            velocity: LpVec3::ZERO,
            pitch: 0,
            yaw: 200,
            head_yaw: 200,
            data: VarInt(0),
        });
        roundtrip(SetEntityMotion {
            id: VarInt(5),
            velocity: LpVec3::ZERO,
        });
        roundtrip(TakeItemEntity {
            item: VarInt(5),
            collector: VarInt(1),
            amount: VarInt(3),
        });
        roundtrip(TeleportEntity {
            id: VarInt(5),
            x: 1.0,
            y: 2.0,
            z: 3.0,
            velocity_x: 0.0,
            velocity_y: -0.1,
            velocity_z: 0.0,
            yaw: 90.0,
            pitch: 0.0,
            relatives: 0,
            on_ground: true,
        });
        let values = [
            (0, MetaValue::Byte(0x20)),
            (1, MetaValue::Int(300)),
            (
                8,
                MetaValue::ItemStack(ItemStack {
                    count: 2,
                    item_id: 28,
                }),
            ),
            (6, MetaValue::Pose(1)),
            (9, MetaValue::Long(-5)),
            (10, MetaValue::String("x".into())),
            (11, MetaValue::Boolean(true)),
            (12, MetaValue::Float(0.5)),
        ];
        roundtrip(SetEntityData {
            id: VarInt(5),
            values: values
                .into_iter()
                .map(|(index, value)| MetaEntry { index, value })
                .collect(),
        });
        // A non-zero velocity survives within LpVec3 precision.
        let mut bytes = Vec::new();
        SetEntityMotion {
            id: VarInt(1),
            velocity: LpVec3([0.1, 0.2, -0.05]),
        }
        .encode(&mut bytes);
        let back = SetEntityMotion::decode(&mut bytes.as_slice()).unwrap();
        assert!((back.velocity.0[1] - 0.2).abs() < 1e-4);
    }
}
