//! Chunk section rewriting: block state and biome IDs inside paletted containers.
//!
//! The section layout (`Short block count | Short fluid count | blocks | biomes`) and the
//! paletted container rules are those of `PalettedContainer` / `Strategy` in the vanilla
//! jar (identical in 26.2 and 26.3):
//!
//! | wire bits | blocks (4096 entries)            | biomes (64 entries)        |
//! |-----------|----------------------------------|----------------------------|
//! | 0         | single value (VarInt)            | single value               |
//! | 1..=4     | palette, stored with 4 bits      | 1..=3: palette, wire bits  |
//! | 5..=8     | palette, stored with wire bits   | -                          |
//! | other     | direct (global IDs), wire bits   | 4+: direct, wire bits      |
//!
//! The data array length is not sent; it follows from the storage bits. Palette entries are
//! remapped in place (the data array is copied verbatim); direct containers are unpacked,
//! remapped and repacked with the bit width of the older version's global palette.

use ferrite_protocol::codec::take;
use ferrite_protocol::{Decode, Encode, VarInt};

use crate::TranslateError;

/// How to rewrite the IDs of one chunk for the target version.
pub struct ChunkRemap<'a> {
    pub block: &'a dyn Fn(u32) -> u32,
    /// Bits of the target version's global block palette (`ceil(log2(state count))`).
    pub block_global_bits: u32,
    pub biome: &'a dyn Fn(u32) -> u32,
    /// Bits of the target client's global biome palette, from the biome registry size it
    /// received; `None` keeps the wire width.
    pub biome_global_bits: Option<u32>,
}

#[derive(Clone, Copy)]
enum Kind {
    Blocks,
    Biomes,
}

impl Kind {
    fn entries(self) -> usize {
        match self {
            Kind::Blocks => 4096,
            Kind::Biomes => 64,
        }
    }

    /// Storage bits of a palette container, or `None` for a direct (global) container.
    fn palette_bits(self, wire: u8) -> Option<u32> {
        match (self, wire) {
            (Kind::Blocks, 1..=4) => Some(4),
            (Kind::Blocks, 5..=8) => Some(u32::from(wire)),
            (Kind::Biomes, 1..=3) => Some(u32::from(wire)),
            _ => None,
        }
    }

    /// Smallest width the client reads as direct.
    fn min_direct_bits(self) -> u32 {
        match self {
            Kind::Blocks => 9,
            Kind::Biomes => 4,
        }
    }
}

/// `ceil(log2(n))`, the vanilla `Mth.ceillog2` used for global palette widths.
pub fn ceil_log2(n: usize) -> u32 {
    if n <= 1 {
        0
    } else {
        usize::BITS - (n - 1).leading_zeros()
    }
}

fn long_count(entries: usize, bits: u32) -> usize {
    if bits == 0 {
        return 0;
    }
    let per_long = 64 / bits as usize;
    entries.div_ceil(per_long)
}

fn unpack(longs: &[u64], bits: u32, entries: usize) -> Vec<u32> {
    let per_long = 64 / bits as usize;
    let mask = (1u64 << bits) - 1;
    (0..entries)
        .map(|i| ((longs[i / per_long] >> ((i % per_long) as u32 * bits)) & mask) as u32)
        .collect()
}

fn pack(values: &[u32], bits: u32) -> Vec<u64> {
    let per_long = 64 / bits as usize;
    let mut longs = vec![0u64; values.len().div_ceil(per_long)];
    for (i, &v) in values.iter().enumerate() {
        longs[i / per_long] |= u64::from(v) << ((i % per_long) as u32 * bits);
    }
    longs
}

fn read_longs(buf: &mut &[u8], n: usize) -> Result<Vec<u64>, TranslateError> {
    let bytes = take(buf, n.checked_mul(8).ok_or(TranslateError::Invalid("data array"))?)?;
    Ok(bytes
        .chunks_exact(8)
        .map(|c| u64::from_be_bytes(c.try_into().unwrap()))
        .collect())
}

fn rewrite_container(
    buf: &mut &[u8],
    out: &mut Vec<u8>,
    kind: Kind,
    map: &dyn Fn(u32) -> u32,
    global_bits: Option<u32>,
) -> Result<(), TranslateError> {
    let wire = u8::decode(buf)?;
    if wire == 0 {
        out.push(0);
        let value = VarInt::decode(buf)?.0 as u32;
        VarInt(map(value) as i32).encode(out);
        return Ok(());
    }
    if let Some(bits) = kind.palette_bits(wire) {
        out.push(wire);
        let palette = Vec::<VarInt>::decode(buf)?;
        VarInt(palette.len() as i32).encode(out);
        for VarInt(v) in palette {
            VarInt(map(v as u32) as i32).encode(out);
        }
        let n = long_count(kind.entries(), bits) * 8;
        out.extend_from_slice(take(buf, n)?);
        return Ok(());
    }
    let bits = u32::from(wire);
    if bits > 32 {
        return Err(TranslateError::Invalid("paletted container bits"));
    }
    let values = unpack(
        &read_longs(buf, long_count(kind.entries(), bits))?,
        bits,
        kind.entries(),
    );
    let values: Vec<u32> = values.into_iter().map(map).collect();
    let target = global_bits
        .unwrap_or(bits)
        .max(kind.min_direct_bits())
        .max(ceil_log2(values.iter().max().map_or(0, |&m| m as usize + 1)));
    out.push(target as u8);
    for long in pack(&values, target) {
        out.extend_from_slice(&long.to_be_bytes());
    }
    Ok(())
}

/// Rewrite the `Data` byte array of a chunk packet: sections until the input ends.
pub fn rewrite_chunk_sections(data: &[u8], remap: &ChunkRemap) -> Result<Vec<u8>, TranslateError> {
    let mut buf = data;
    let mut out = Vec::with_capacity(data.len());
    while !buf.is_empty() {
        out.extend_from_slice(take(&mut buf, 4)?); // block count, fluid count
        rewrite_container(
            &mut buf,
            &mut out,
            Kind::Blocks,
            remap.block,
            Some(remap.block_global_bits),
        )?;
        rewrite_container(
            &mut buf,
            &mut out,
            Kind::Biomes,
            remap.biome,
            remap.biome_global_bits,
        )?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceil_log2_matches_vanilla() {
        assert_eq!(ceil_log2(1), 0);
        assert_eq!(ceil_log2(2), 1);
        assert_eq!(ceil_log2(64), 6);
        assert_eq!(ceil_log2(65), 7);
        assert_eq!(ceil_log2(32366), 15);
        assert_eq!(ceil_log2(35723), 16);
    }

    #[test]
    fn pack_unpack_roundtrip_with_padding() {
        let values: Vec<u32> = (0..4096).map(|i| (i * 7919) % 35723).collect();
        for bits in [15, 16] {
            let longs = pack(&values, bits);
            assert_eq!(longs.len(), long_count(4096, bits));
            assert_eq!(unpack(&longs, bits, 4096), values);
        }
    }
}
