//! Packet definitions, one module per connection state.
//!
//! Packet IDs come from the vanilla data generator report (`packets.json`) for
//! [`crate::GAME_VERSION`]; see `crates/ferrite-data`.

pub mod handshake;
pub mod status;
#[cfg(test)]
mod login_tests {
    use super::login::*;
    use crate::{Decode, Encode};
    fn roundtrip<T: Encode + Decode + PartialEq + std::fmt::Debug>(value: T) {
        let mut out = Vec::new();
        value.encode(&mut out);
        let mut data = out.as_slice();
        assert_eq!(T::decode(&mut data).unwrap(), value);
        assert!(data.is_empty());
    }
    #[test]
    fn login_packet_roundtrips() {
        roundtrip(LoginStart {
            name: "Steve".into(),
            uuid: uuid::Uuid::nil(),
        });
        roundtrip(EncryptionRequest {
            server_id: "".into(),
            public_key: vec![1, 2],
            verify_token: vec![3],
            should_authenticate: false,
        });
        roundtrip(EncryptionResponse {
            shared_secret: vec![4],
            verify_token: vec![3],
        });
        roundtrip(LoginSuccess {
            uuid: uuid::Uuid::nil(),
            name: "Steve".into(),
            properties: vec![],
            session_id: uuid::Uuid::nil(),
        });
        roundtrip(SetCompression {
            threshold: crate::VarInt(256),
        });
        roundtrip(LoginAcknowledged);
        roundtrip(LoginDisconnect {
            reason: "bye".into(),
        });
        roundtrip(CustomQueryAnswer {
            message_id: crate::VarInt(1),
            data: Some(crate::RestBytes(vec![3, 4])),
        });
    }
}
pub mod login;
#[cfg(test)]
mod configuration_tests {
    use super::configuration::*;
    use crate::{Decode, Encode, RestBytes, VarInt};
    fn roundtrip<T: Encode + Decode + PartialEq + std::fmt::Debug>(v: T) {
        let mut b = Vec::new();
        v.encode(&mut b);
        let mut s = b.as_slice();
        assert_eq!(T::decode(&mut s).unwrap(), v);
        assert!(s.is_empty());
    }
    #[test]
    fn configuration_packets_roundtrip() {
        let pack = KnownPack {
            namespace: "minecraft".into(),
            id: "core".into(),
            version: "26.3".into(),
        };
        roundtrip(ClientboundKnownPacks {
            packs: vec![pack.clone()],
        });
        roundtrip(ServerboundKnownPacks { packs: vec![pack] });
        roundtrip(RegistryData {
            registry: "minecraft:dimension_type".into(),
            entries: vec!["minecraft:overworld".into()],
        });
        roundtrip(UpdateTags {
            registries: vec![RegistryTags {
                registry: "minecraft:block".into(),
                tags: vec![Tag {
                    name: "minecraft:test".into(),
                    entries: vec![VarInt(1)],
                }],
            }],
        });
        roundtrip(FinishConfiguration);
        roundtrip(AcknowledgeFinishConfiguration);
        roundtrip(ClientboundCustomPayload {
            channel: "minecraft:brand".into(),
            data: RestBytes(vec![1]),
        });
        roundtrip(ClientInformation {
            locale: "en_us".into(),
            view_distance: 8,
            chat_mode: VarInt(0),
            chat_colors: true,
            skin_parts: 127,
            main_hand: VarInt(1),
            text_filtering: false,
            server_listing: true,
            particle_status: VarInt(0),
        });
        roundtrip(ServerboundCustomPayload {
            channel: "minecraft:brand".into(),
            data: RestBytes(vec![1]),
        });
        roundtrip(ConfigurationDisconnect {
            reason: crate::types::PlainText("bye".into()),
        });
        roundtrip(ClientboundKeepAlive { id: 42 });
        roundtrip(ServerboundKeepAlive { id: 42 });
    }
}
pub mod configuration;
#[cfg(test)]
mod play_tests {
    use super::play::*;
    use crate::{Decode, Encode, VarInt};
    fn rt<T: Encode + Decode + PartialEq + std::fmt::Debug>(v: T) {
        let mut b = Vec::new();
        v.encode(&mut b);
        let mut s = b.as_slice();
        assert_eq!(T::decode(&mut s).unwrap(), v);
        assert!(s.is_empty());
    }
    #[test]
    fn play_packet_roundtrips() {
        rt(ConfirmTeleport {
            id: VarInt(1),
            x: 0.5,
            y: 65.0,
            z: 0.5,
            yaw: 0.0,
            pitch: 0.0,
        });
        rt(MovePosition {
            x: 0.5,
            y: 65.0,
            z: 0.5,
            flags: 1,
        });
        rt(MovePositionRotation {
            x: 0.5,
            y: 65.0,
            z: 0.5,
            yaw: 0.0,
            pitch: 0.0,
            flags: 1,
        });
        rt(MoveRotation {
            yaw: 0.0,
            pitch: 0.0,
            flags: 1,
        });
        rt(MoveFlags { flags: 1 });
        rt(PlayerLoaded);
        rt(PlayDisconnect {
            reason: crate::types::PlainText("bye".into()),
        });
        rt(GameEvent {
            event: 13,
            value: 0.0,
        });
        rt(ChunkBatchStart);
        rt(ChunkBatchFinished { size: VarInt(1) });
        rt(CenterChunk {
            x: VarInt(0),
            z: VarInt(0),
        });
        rt(PositionSync {
            teleport_id: VarInt(1),
            x: 0.5,
            y: 65.0,
            z: 0.5,
            velocity_x: 0.0,
            velocity_y: 0.0,
            velocity_z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            flags: 0,
        });
        rt(DefaultSpawn {
            dimension: "minecraft:overworld".into(),
            position: 65,
            yaw: 0.0,
            pitch: 0.0,
        });
        rt(PlayKeepAlive { id: 5 });
        rt(ClientKeepAlive { id: 5 });
        rt(ChunkData {
            x: 0,
            z: 0,
            heightmaps: vec![],
            data: vec![1, 2],
            sky_mask: vec![],
            block_mask: vec![],
            empty_sky_mask: vec![1],
            empty_block_mask: vec![1],
            sky_arrays: vec![],
            block_arrays: vec![],
        });
        rt(PlayerInfo {
            uuid: uuid::Uuid::nil(),
            name: "Steve".into(),
            properties: vec![],
            chat_session: None,
            game_mode: VarInt(1),
        });
        rt(PlayLogin {
            entity_id: 1,
            hardcore: false,
            dimensions: vec!["minecraft:overworld".into()],
            max_players: VarInt(20),
            view_distance: VarInt(2),
            simulation_distance: VarInt(2),
            reduced_debug: false,
            respawn_screen: true,
            limited_crafting: false,
            dimension_type: VarInt(0),
            dimension: "minecraft:overworld".into(),
            seed: 0,
            game_mode: VarInt(0),
            previous_game_mode: VarInt(0),
            debug: false,
            flat: true,
            death_location: None,
            portal_cooldown: VarInt(0),
            sea_level: VarInt(63),
            online_mode: false,
            secure_chat: false,
        });
    }
}
pub mod play;
#[cfg(test)]
mod multiplayer_tests {
    use super::play::*;
    use crate::{Decode, Encode, VarInt, types::PlainText};
    fn rt<T: Encode + Decode + PartialEq + std::fmt::Debug>(x: T) {
        let mut b = Vec::new();
        x.encode(&mut b);
        let mut s = b.as_slice();
        assert_eq!(T::decode(&mut s).unwrap(), x);
        assert!(s.is_empty());
    }

    #[test]
    fn player_info_marks_players_listed() {
        let info = PlayerInfo {
            uuid: uuid::Uuid::nil(),
            name: "Steve".into(),
            properties: vec![],
            chat_session: None,
            game_mode: VarInt(1),
        };
        let mut b = Vec::new();
        info.encode(&mut b);
        assert_eq!(b[0], 15);
        assert_eq!(b[b.len() - 1], 1);
    }
    #[test]
    fn multiplayer_packet_roundtrips() {
        rt(AddEntity {
            id: VarInt(1),
            uuid: uuid::Uuid::nil(),
            entity_type: VarInt(159),
            x: 0.5,
            y: 65.0,
            z: 0.5,
            velocity: crate::types::LpVec3::ZERO,
            pitch: 0,
            yaw: 0,
            head_yaw: 0,
            data: VarInt(0),
        });
        rt(EntityMove {
            id: VarInt(1),
            properties: VarInt(1),
            dx: 2,
            dy: 0,
            dz: 0,
        });
        rt(EntityMoveRotation {
            id: VarInt(1),
            properties: VarInt(1),
            dx: 2,
            dy: 0,
            dz: 0,
            yaw: 2,
            pitch: 3,
        });
        rt(EntityRotation {
            id: VarInt(1),
            on_ground: true,
            yaw: 2,
            pitch: 3,
        });
        rt(EntityHeadRotation {
            id: VarInt(1),
            yaw: 2,
        });
        rt(EntityTeleport {
            id: VarInt(1),
            path_type: VarInt(0),
            x: 1.0,
            y: 65.0,
            z: 2.0,
            yaw: 0.0,
            pitch: 0.0,
            on_ground: true,
        });
        rt(ForgetChunk { z: 1, x: -2 });
        rt(RemoveEntities {
            ids: vec![VarInt(1)],
        });
        rt(PlayerInfoRemove {
            uuids: vec![uuid::Uuid::nil()],
        });
        rt(SystemChat {
            content: PlainText("hello".into()),
            overlay: false,
        });
        rt(ChunkBatchReceived {
            chunks_per_tick: 3.0,
        });
    }
}
