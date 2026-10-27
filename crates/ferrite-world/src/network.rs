use crate::{Chunk, HeightmapKind, put_varint};
use ferrite_nbt::{Tag, binary};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LightSection {
    Unknown,
    Empty,
    Data(Box<[u8; 2048]>),
}
#[derive(Debug, Clone, Default)]
pub struct LightData {
    pub sky: Vec<LightSection>,
    pub block: Vec<LightSection>,
}
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("light section count does not match dimension")]
    LightCount,
    #[error("block entity Y out of range")]
    EntityY,
    #[error(transparent)]
    Nbt(#[from] binary::Error),
}
/// Encode the complete packet body, including coordinates and light masks.
pub fn encode_chunk_packet(
    x: i32,
    z: i32,
    chunk: &Chunk,
    light: &LightData,
) -> Result<Vec<u8>, NetworkError> {
    let count = chunk.sections().len() + 2;
    if light.sky.len() != count || light.block.len() != count {
        return Err(NetworkError::LightCount);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&x.to_be_bytes());
    out.extend_from_slice(&z.to_be_bytes());
    put_varint(&mut out, HeightmapKind::ALL.len() as u32);
    let bits = (u32::BITS - chunk.height_blocks().leading_zeros()) as usize;
    let per = 64 / bits;
    for kind in HeightmapKind::ALL {
        put_varint(&mut out, kind as u32);
        let mut words = vec![0u64; 256usize.div_ceil(per)];
        for (i, &value) in chunk.heights(kind).iter().enumerate() {
            words[i / per] |= u64::from(value) << ((i % per) * bits);
        }
        put_varint(&mut out, words.len() as u32);
        for word in words {
            out.extend_from_slice(&word.to_be_bytes());
        }
    }
    let mut sections = Vec::new();
    for section in chunk.sections() {
        sections.extend(section.encode_network());
    }
    put_varint(&mut out, sections.len() as u32);
    out.extend(sections);
    put_varint(&mut out, chunk.block_entities.len() as u32);
    for (&(x, y, z), entity) in &chunk.block_entities {
        if x >= 16 || z >= 16 {
            return Err(NetworkError::EntityY);
        }
        out.push((x << 4) | z);
        out.extend_from_slice(
            &i16::try_from(y)
                .map_err(|_| NetworkError::EntityY)?
                .to_be_bytes(),
        );
        put_varint(&mut out, entity.type_id);
        let mut data = entity.data.clone();
        data.shift_remove("x");
        data.shift_remove("y");
        data.shift_remove("z");
        out.extend(binary::write_network(&Tag::Compound(data))?);
    }
    write_mask(&mut out, &light.sky, false);
    write_mask(&mut out, &light.block, false);
    write_mask(&mut out, &light.sky, true);
    write_mask(&mut out, &light.block, true);
    write_arrays(&mut out, &light.sky);
    write_arrays(&mut out, &light.block);
    Ok(out)
}
fn write_mask(out: &mut Vec<u8>, sections: &[LightSection], empty: bool) {
    let mut words = vec![0u64; sections.len().div_ceil(64)];
    for (i, section) in sections.iter().enumerate() {
        if if empty {
            matches!(section, LightSection::Empty)
        } else {
            matches!(section, LightSection::Data(_))
        } {
            words[i / 64] |= 1u64 << (i % 64);
        }
    }
    while words.last() == Some(&0) {
        words.pop();
    }
    put_varint(out, words.len() as u32);
    for word in words {
        out.extend_from_slice(&word.to_be_bytes());
    }
}
fn write_arrays(out: &mut Vec<u8>, sections: &[LightSection]) {
    put_varint(
        out,
        sections
            .iter()
            .filter(|s| matches!(s, LightSection::Data(_)))
            .count() as u32,
    );
    for section in sections {
        if let LightSection::Data(data) = section {
            put_varint(out, 2048);
            out.extend_from_slice(&data[..]);
        }
    }
}
