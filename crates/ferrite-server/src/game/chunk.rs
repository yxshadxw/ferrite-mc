use ferrite_protocol::{
    Encode, VarInt,
    packets::play::{ChunkData, Heightmap},
};
use std::collections::HashMap;
/// Supplies wire-format chunk columns; world-backed implementations can replace this generator.
pub trait ChunkSource {
    fn get(&mut self, x: i32, z: i32, biome: i32) -> ChunkData;
}
#[derive(Default)]
pub struct FlatGenerator {
    cache: HashMap<(i32, i32, i32), ChunkData>,
}
impl FlatGenerator {
    pub fn merge(&mut self, other: Self) {
        self.cache.extend(other.cache);
    }
    pub fn split(
        &mut self,
        sections: &std::collections::HashSet<ferrite_region::SectionPos>,
    ) -> Self {
        let keys: Vec<_> = self
            .cache
            .keys()
            .copied()
            .filter(|(x, z, _)| sections.contains(&ferrite_region::SectionPos(*x >> 4, *z >> 4)))
            .collect();
        let mut out = Self::default();
        for key in keys {
            if let Some(value) = self.cache.remove(&key) {
                out.cache.insert(key, value);
            }
        }
        out
    }
}
impl ChunkSource for FlatGenerator {
    fn get(&mut self, x: i32, z: i32, biome: i32) -> ChunkData {
        self.cache
            .entry((x, z, biome))
            .or_insert_with(|| generate(x, z, biome))
            .clone()
    }
}
fn generate(x: i32, z: i32, biome: i32) -> ChunkData {
    let mut sections = Vec::new();
    for section in 0..24 {
        (match section {
            7 => 768i16,
            8 => 256i16,
            _ => 0i16,
        })
        .encode(&mut sections);
        0i16.encode(&mut sections);
        if section == 7 || section == 8 {
            4u8.encode(&mut sections);
            if section == 7 {
                vec![VarInt(0), VarInt(88), VarInt(10)].encode(&mut sections);
            } else {
                vec![VarInt(0), VarInt(9)].encode(&mut sections);
            }
            for i in 0..256 {
                let layer = i / 16;
                let v = if section == 7 && layer == 13 {
                    1i64
                } else if section == 7 && layer >= 14 {
                    2i64
                } else if section == 8 && layer == 0 {
                    1i64
                } else {
                    0
                };
                (v * 0x1111_1111_1111_1111).encode(&mut sections);
            }
        } else {
            0u8.encode(&mut sections);
            VarInt(0).encode(&mut sections);
        }
        0u8.encode(&mut sections);
        VarInt(biome).encode(&mut sections);
    }
    let mut heights = vec![0i64; 37];
    for i in 0..256 {
        heights[i / 7] |= 129i64 << ((i % 7) * 9);
    }
    ChunkData {
        x,
        z,
        heightmaps: vec![
            Heightmap {
                kind: VarInt(1),
                data: heights.clone(),
            },
            Heightmap {
                kind: VarInt(4),
                data: heights,
            },
        ],
        data: sections,
        sky_mask: vec![],
        block_mask: vec![],
        empty_sky_mask: vec![(1 << 26) - 1],
        empty_block_mask: vec![(1 << 26) - 1],
        sky_arrays: vec![],
        block_arrays: vec![],
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_protocol::Decode;
    #[test]
    fn flat_generator_builds_bedrock_dirt_and_grass() {
        let mut g = FlatGenerator::default();
        let chunk = g.get(0, 0, 0);
        let mut input = chunk.data.as_slice();
        for section in 0..24 {
            let count = i16::decode(&mut input).unwrap();
            assert_eq!(
                count,
                if section == 7 {
                    768
                } else if section == 8 {
                    256
                } else {
                    0
                }
            );
            let _fluid = i16::decode(&mut input).unwrap();
            let bits = u8::decode(&mut input).unwrap();
            if bits == 0 {
                ferrite_protocol::VarInt::decode(&mut input).unwrap();
            } else {
                let n = ferrite_protocol::VarInt::decode(&mut input).unwrap().0;
                for _ in 0..n {
                    ferrite_protocol::VarInt::decode(&mut input).unwrap();
                }
                for _ in 0..256 {
                    i64::decode(&mut input).unwrap();
                }
            }
            assert_eq!(u8::decode(&mut input).unwrap(), 0);
            ferrite_protocol::VarInt::decode(&mut input).unwrap();
        }
        assert!(input.is_empty());
    }
}
