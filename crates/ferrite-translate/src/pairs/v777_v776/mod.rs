//! 26.3 (protocol 777) -> 26.2 (protocol 776).
//!
//! The two versions differ mostly by packet IDs (104 play clientbound IDs moved) and by
//! four packets that only exist in 26.3. We remap IDs by packet name, drop packets the
//! older client cannot decode, and rewrite the bodies whose layout or contents changed:
//! the handshake protocol field, known-pack versions, synchronized registries and tags,
//! and chunk sections/light masks. Registry IDs inside the remaining play packets are the
//! responsibility of the packet implementations that carry them.

use ferrite_protocol::packets::play::ChunkData;
use ferrite_protocol::{Bound, Decode, Encode, State, VarInt};

use crate::tables::PairTables;
use crate::translator::{ProtocolVersion, Step, VersionPair};
use crate::{CORE_VERSION, RawPacket, TranslateError, chunk, common, registry_sync, tags};

include!(concat!(env!("OUT_DIR"), "/v777_v776.rs"));

pub static TABLES: PairTables = PairTables {
    new_packets: &NEW_PACKETS,
    old_packets: &OLD_PACKETS,
    packets_down: &PACKETS_DOWN,
    packets_up: &PACKETS_UP,
    block_states_down: &BLOCK_STATES_DOWN,
    block_states_up: &BLOCK_STATES_UP,
    registries: REGISTRIES,
    old_static_registries: OLD_STATIC_REGISTRIES,
    old_data_registries: OLD_DATA_REGISTRIES,
    fallbacks: FALLBACKS,
};

pub static PAIR: VersionPair = VersionPair {
    newer: CORE_VERSION,
    older: ProtocolVersion {
        protocol: 776,
        name: "26.2",
    },
    tables: &TABLES,
    session: || Box::new(Session::default()),
};

#[derive(Default)]
struct Session {
    registries: registry_sync::DynamicRegistries,
}

impl Session {
    fn down_id(&self, state: State, bound: Bound, id: i32) -> Option<i32> {
        TABLES.packet_down(state, bound, id)
    }

    fn up_id(&self, state: State, bound: Bound, id: i32) -> Option<i32> {
        TABLES.packet_up(state, bound, id)
    }
}

impl Step for Session {
    fn clientbound(
        &mut self,
        state: State,
        packet: RawPacket,
        out: &mut Vec<RawPacket>,
    ) -> Result<(), TranslateError> {
        let name = TABLES
            .new_packet_name(state, Bound::Clientbound, packet.id)
            .ok_or(TranslateError::UnknownPacket {
                protocol: CORE_VERSION.protocol,
                state,
                bound: Bound::Clientbound,
                id: packet.id,
            })?;
        match (state, name) {
            // 26.3-only packets: no older equivalent.
            (State::Configuration, "minecraft:post_effects")
            | (
                State::Play,
                "minecraft:add_transient_block"
                | "minecraft:post_effects"
                | "minecraft:swing_animation",
            ) => Ok(()),
            (State::Status, "minecraft:status_response") => {
                let body = common::rewrite_status(&packet.body, 777, 776)?;
                let Some(id) = self.down_id(state, Bound::Clientbound, packet.id) else {
                    return Err(unknown(state, Bound::Clientbound, packet.id));
                };
                out.push(RawPacket::new(id, body));
                Ok(())
            }
            (State::Configuration, "minecraft:select_known_packs") => {
                let body = common::rewrite_known_packs(&packet.body, "26.3", "26.2")?;
                self.push_mapped(state, Bound::Clientbound, packet.id, body, out)
            }
            (State::Configuration, "minecraft:registry_data") => {
                match registry_sync::rewrite_registry_data(
                    &packet.body,
                    &TABLES,
                    &mut self.registries,
                )? {
                    Some(body) => self.push_mapped(state, Bound::Clientbound, packet.id, body, out),
                    None => Ok(()),
                }
            }
            (State::Configuration | State::Play, "minecraft:update_tags") => {
                let body = tags::rewrite_update_tags(&packet.body, &TABLES, &self.registries)?;
                self.push_mapped(state, Bound::Clientbound, packet.id, body, out)
            }
            (State::Play, "minecraft:level_chunk_with_light") => {
                let body = rewrite_chunk(&packet.body, &self.registries)?;
                self.push_mapped(state, Bound::Clientbound, packet.id, body, out)
            }
            _ => self.push_mapped(state, Bound::Clientbound, packet.id, packet.body, out),
        }
    }

    fn serverbound(
        &mut self,
        state: State,
        packet: RawPacket,
        out: &mut Vec<RawPacket>,
    ) -> Result<(), TranslateError> {
        let name = TABLES.old_packet_name(state, Bound::Serverbound, packet.id);
        match (state, name) {
            (State::Handshaking, _) if packet.id == 0 => {
                let body = common::rewrite_intention(&packet.body, 776, 777)?;
                out.push(RawPacket::new(0, body));
                Ok(())
            }
            (State::Status, _) => {
                out.push(RawPacket::new(
                    self.up_id_or_err(state, packet.id)?,
                    packet.body,
                ));
                Ok(())
            }
            (State::Configuration, Some("minecraft:select_known_packs")) => {
                let body = common::rewrite_known_packs(&packet.body, "26.2", "26.3")?;
                self.push_mapped_up(state, packet.id, body, out)
            }
            // 26.2 calls the swing packet `swing`; 26.3 renamed it to `punch`.
            (State::Play, Some("minecraft:swing")) => {
                let id = TABLES
                    .new_packet_id(state, Bound::Serverbound, "minecraft:punch")
                    .ok_or(TranslateError::Invalid("missing punch packet"))?;
                out.push(RawPacket::new(id, packet.body));
                Ok(())
            }
            _ => self.push_mapped_up(state, packet.id, packet.body, out),
        }
    }
}

fn unknown(state: State, bound: Bound, id: i32) -> TranslateError {
    TranslateError::UnknownPacket {
        protocol: CORE_VERSION.protocol,
        state,
        bound,
        id,
    }
}

impl Session {
    fn push_mapped(
        &self,
        state: State,
        bound: Bound,
        id: i32,
        body: Vec<u8>,
        out: &mut Vec<RawPacket>,
    ) -> Result<(), TranslateError> {
        let id = self
            .down_id(state, bound, id)
            .ok_or_else(|| unknown(state, bound, id))?;
        out.push(RawPacket::new(id, body));
        Ok(())
    }

    fn up_id_or_err(&self, state: State, id: i32) -> Result<i32, TranslateError> {
        self.up_id(state, Bound::Serverbound, id)
            .ok_or_else(|| unknown(state, Bound::Serverbound, id))
    }

    fn push_mapped_up(
        &self,
        state: State,
        id: i32,
        body: Vec<u8>,
        out: &mut Vec<RawPacket>,
    ) -> Result<(), TranslateError> {
        out.push(RawPacket::new(self.up_id_or_err(state, id)?, body));
        Ok(())
    }
}

/// Rewrite the section data and the 26.3-specific light bitset encoding.
fn rewrite_chunk(
    body: &[u8],
    registries: &registry_sync::DynamicRegistries,
) -> Result<Vec<u8>, TranslateError> {
    let mut buf = body;
    let chunk = ChunkData::decode(&mut buf)?;
    if !buf.is_empty() {
        return Err(TranslateError::Invalid("trailing bytes in chunk packet"));
    }
    let biome = registries.get("minecraft:worldgen/biome").cloned();
    let block_remap = |id: u32| TABLES.block_state_down(id);
    let biome_remap = |id: u32| match &biome {
        Some(remap) => remap.down(id as i32).unwrap_or(0) as u32,
        None => id,
    };
    let biome_bits = biome
        .as_ref()
        .map(|remap| chunk::ceil_log2(remap.old_len().max(1)));
    let data = chunk::rewrite_chunk_sections(
        &chunk.data,
        &chunk::ChunkRemap {
            block: &block_remap,
            block_global_bits: chunk::ceil_log2(TABLES.old_block_state_count().max(1)),
            biome: &biome_remap,
            biome_global_bits: biome_bits,
        },
    )?;

    // 26.2 reads the four light masks as VarInt-length long arrays; 26.3 uses a
    // VarInt-length little-endian byte array (see `ChunkData` in ferrite-protocol).
    let mut out = Vec::with_capacity(body.len());
    chunk.x.encode(&mut out);
    chunk.z.encode(&mut out);
    chunk.heightmaps.encode(&mut out);
    data.encode(&mut out);
    VarInt(0).encode(&mut out);
    for mask in [
        &chunk.sky_mask,
        &chunk.block_mask,
        &chunk.empty_sky_mask,
        &chunk.empty_block_mask,
    ] {
        mask.clone().encode(&mut out);
    }
    chunk.sky_arrays.encode(&mut out);
    chunk.block_arrays.encode(&mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_loaded_from_the_reports() {
        assert_eq!(
            TABLES.new_packet_id(State::Play, Bound::Serverbound, "minecraft:punch"),
            Some(46)
        );
        assert_eq!(
            TABLES.old_packet_id(State::Play, Bound::Serverbound, "minecraft:swing"),
            Some(63)
        );
        assert!(TABLES.old_block_state_count() > 0);
        assert!(TABLES.new_block_state_count() > TABLES.old_block_state_count());
    }

    #[test]
    fn punch_and_swing_map_both_ways() {
        let mut session = Session::default();
        let mut out = Vec::new();
        session
            .serverbound(State::Play, RawPacket::new(63, Vec::new()), &mut out)
            .unwrap();
        assert_eq!(out[0].id, 46);
    }
}
