//! End-to-end checks of the 777 -> 776 pair: every packet ID round trip, handshake
//! negotiation, and the body rewrites that carry registry/tag/chunk data.

use ferrite_protocol::packets::configuration::{
    KnownPack, RegistryData, RegistryTags, Tag, UpdateTags,
};
use ferrite_protocol::packets::handshake::{Intent, Intention};
use ferrite_protocol::{Bound, Decode, Encode, State, VarInt};
use ferrite_translate::pairs::v777_v776::TABLES;
use ferrite_translate::tables::slot;
use ferrite_translate::{
    CORE_VERSION, HandshakeOutcome, RawPacket, Translator, supported_versions,
};

const STATES: [(State, &str); 5] = [
    (State::Handshaking, "handshake"),
    (State::Status, "status"),
    (State::Login, "login"),
    (State::Configuration, "configuration"),
    (State::Play, "play"),
];

#[test]
fn every_packet_id_maps_down_and_back() {
    for (state, _) in STATES {
        for bound in [Bound::Serverbound, Bound::Clientbound] {
            let names = TABLES.new_packets[slot(state, bound)];
            for (new_id, name) in names.iter().enumerate() {
                if name.is_empty() {
                    continue;
                }
                let Some(old_id) = TABLES.packet_down(state, bound, new_id as i32) else {
                    // 26.3-only packet: no older client can decode it.
                    continue;
                };
                assert!(
                    TABLES.packet_up(state, bound, old_id).is_some(),
                    "{name} {state:?}/{bound:?} maps down to {old_id} but not back"
                );
            }
        }
        for bound in [Bound::Serverbound, Bound::Clientbound] {
            let names = TABLES.old_packets[slot(state, bound)];
            for (old_id, name) in names.iter().enumerate() {
                if name.is_empty() {
                    continue;
                }
                let Some(new_id) = TABLES.packet_up(state, bound, old_id as i32) else {
                    continue;
                };
                assert!(
                    TABLES.packet_down(state, bound, new_id).is_some(),
                    "{name} {state:?}/{bound:?} maps up to {new_id} but not back"
                );
            }
        }
    }
}

#[test]
fn new_only_packets_are_the_known_four() {
    let mut missing = Vec::new();
    for (state, _) in STATES {
        for bound in [Bound::Serverbound, Bound::Clientbound] {
            for (id, name) in TABLES.new_packets[slot(state, bound)].iter().enumerate() {
                if !name.is_empty() && TABLES.packet_down(state, bound, id as i32).is_none() {
                    missing.push(*name);
                }
            }
        }
    }
    missing.sort_unstable();
    missing.dedup();
    assert_eq!(
        missing,
        [
            "minecraft:add_transient_block",
            "minecraft:post_effects",
            "minecraft:punch",
            "minecraft:swing_animation"
        ],
        "26.3-only packets changed; update the pair's drop/alias list"
    );
}

#[test]
fn handshake_776_is_rewritten_for_the_core() {
    let mut body = Vec::new();
    Intention {
        protocol_version: 776,
        server_address: "localhost".into(),
        server_port: 25565,
        intent: Intent::Login,
    }
    .encode(&mut body);
    match Translator::from_handshake(&RawPacket::new(0, body)).unwrap() {
        HandshakeOutcome::Supported {
            translator,
            forwarded,
            intention,
        } => {
            assert_eq!(translator.client_version().protocol, 776);
            assert!(!translator.is_passthrough());
            assert_eq!(intention.protocol_version, 776);
            let rewritten = Intention::decode(&mut forwarded.body.as_slice()).unwrap();
            assert_eq!(rewritten.protocol_version, CORE_VERSION.protocol);
        }
        HandshakeOutcome::Unsupported { .. } => panic!("776 must be supported"),
    }
}

#[test]
fn native_client_is_passthrough_and_unknown_version_is_rejected() {
    let builder = Translator::new(CORE_VERSION.protocol).unwrap();
    assert!(builder.is_passthrough());
    assert!(Translator::new(775).is_err());
    assert_eq!(supported_versions().len(), 2);
}

#[test]
fn serverbound_swing_becomes_punch() {
    let mut translator = Translator::new(776).unwrap();
    let packets = translator
        .serverbound(State::Play, RawPacket::new(63, Vec::new()))
        .unwrap();
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].id, 46);
}

#[test]
fn known_pack_version_is_rewritten_both_ways() {
    let pack = KnownPack {
        namespace: "minecraft".into(),
        id: "core".into(),
        version: "26.3".into(),
    };
    let mut body = Vec::new();
    vec![pack].encode(&mut body);
    let mut translator = Translator::new(776).unwrap();
    let down = translator
        .clientbound(State::Configuration, RawPacket::new(15, body))
        .unwrap();
    let packs = Vec::<KnownPack>::decode(&mut down[0].body.as_slice()).unwrap();
    assert_eq!(packs[0].version, "26.2");
    assert_eq!(down[0].id, 14);

    let mut up = translator
        .serverbound(
            State::Configuration,
            RawPacket::new(7, down[0].body.clone()),
        )
        .unwrap();
    let packs = Vec::<KnownPack>::decode(&mut up[0].body.as_slice()).unwrap();
    assert_eq!(packs[0].version, "26.3");
    assert_eq!(up[0].id, 7);
    up.clear();
}

#[test]
fn registry_data_drops_entries_the_older_version_lacks() {
    let data = RegistryData {
        registry: "minecraft:worldgen/biome".into(),
        entries: vec![
            "minecraft:plains".into(),
            "minecraft:not_a_real_biome".into(),
        ],
    };
    let mut body = Vec::new();
    data.encode(&mut body);
    let mut translator = Translator::new(776).unwrap();
    let packet = translator
        .clientbound(State::Configuration, RawPacket::new(7, body))
        .unwrap();
    assert_eq!(packet.len(), 1);
    let decoded = RegistryData::decode(&mut packet[0].body.as_slice()).unwrap();
    assert_eq!(decoded.entries, ["minecraft:plains"]);
}

#[test]
fn tags_drop_unmappable_entries() {
    let table = TABLES
        .registry("minecraft:block")
        .expect("block registry table");
    let (new_id, _) = table
        .down
        .iter()
        .enumerate()
        .find(|(_, id)| **id >= 0)
        .expect("at least one block maps");
    let tags = UpdateTags {
        registries: vec![RegistryTags {
            registry: "minecraft:block".into(),
            tags: vec![Tag {
                name: "minecraft:mineable/pickaxe".into(),
                entries: vec![VarInt(new_id as i32)],
            }],
        }],
    };
    let mut body = Vec::new();
    tags.encode(&mut body);
    let mut translator = Translator::new(776).unwrap();
    let packets = translator
        .clientbound(State::Configuration, RawPacket::new(14, body))
        .unwrap();
    let decoded = UpdateTags::decode(&mut packets[0].body.as_slice()).unwrap();
    let entries = &decoded.registries[0].tags[0].entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, table.down[new_id]);
}

#[test]
fn chunk_light_masks_use_the_262_long_array_format() {
    use ferrite_protocol::packets::play::ChunkData;
    let mut data = Vec::new();
    0i16.encode(&mut data);
    0i16.encode(&mut data);
    0u8.encode(&mut data);
    VarInt(1).encode(&mut data);
    0u8.encode(&mut data);
    VarInt(0).encode(&mut data);
    let chunk = ChunkData {
        x: 0,
        z: 0,
        heightmaps: vec![],
        data,
        sky_mask: vec![1],
        block_mask: vec![],
        empty_sky_mask: vec![0],
        empty_block_mask: vec![0],
        sky_arrays: vec![],
        block_arrays: vec![],
    };
    let mut body = Vec::new();
    chunk.encode(&mut body);
    let mut translator = Translator::new(776).unwrap();
    let packets = translator
        .clientbound(State::Play, RawPacket::new(46, body))
        .unwrap();
    let mut buf = packets[0].body.as_slice();
    let _x = i32::decode(&mut buf).unwrap();
    let _z = i32::decode(&mut buf).unwrap();
    let _heightmaps = Vec::<ferrite_protocol::packets::play::Heightmap>::decode(&mut buf).unwrap();
    let _data = Vec::<u8>::decode(&mut buf).unwrap();
    assert_eq!(VarInt::decode(&mut buf).unwrap().0, 0);
    assert_eq!(Vec::<i64>::decode(&mut buf).unwrap(), [1]);
    assert!(Vec::<i64>::decode(&mut buf).unwrap().is_empty());
    // All-zero longs are no bits set: a `BitSet` serializes as an empty long array.
    assert!(Vec::<i64>::decode(&mut buf).unwrap().is_empty());
    assert!(Vec::<i64>::decode(&mut buf).unwrap().is_empty());
    assert!(Vec::<Vec<u8>>::decode(&mut buf).unwrap().is_empty());
    assert!(Vec::<Vec<u8>>::decode(&mut buf).unwrap().is_empty());
    assert!(buf.is_empty());
}
