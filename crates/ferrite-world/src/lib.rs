//! Chunk storage primitives.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteKind {
    Blocks,
    Biomes,
}
impl PaletteKind {
    pub const fn len(self) -> usize {
        match self {
            Self::Blocks => 4096,
            Self::Biomes => 64,
        }
    }
    pub const fn is_empty(self) -> bool {
        false
    }
    const fn min_bits(self) -> u8 {
        match self {
            Self::Blocks => 4,
            Self::Biomes => 1,
        }
    }
    const fn max_bits(self) -> u8 {
        match self {
            Self::Blocks => 8,
            Self::Biomes => 3,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PaletteError {
    #[error("invalid global bit width")]
    InvalidBits,
    #[error("index out of bounds")]
    Index,
    #[error("value does not fit global bit width")]
    Value,
}

/// Packed entries use the Java layout: no entry crosses a 64-bit word.
#[derive(Debug, Clone)]
pub struct PalettedContainer {
    kind: PaletteKind,
    global_bits: u8,
    bits: u8,
    palette: Vec<u32>,
    counts: BTreeMap<u32, usize>,
    data: Vec<u64>,
}
impl PalettedContainer {
    pub fn new(kind: PaletteKind, value: u32, global_bits: u8) -> Result<Self, PaletteError> {
        if global_bits <= kind.max_bits() || global_bits > 32 {
            return Err(PaletteError::InvalidBits);
        }
        if u64::from(value) >= (1u64 << global_bits) {
            return Err(PaletteError::Value);
        }
        Ok(Self {
            kind,
            global_bits,
            bits: 0,
            palette: vec![value],
            counts: BTreeMap::from([(value, kind.len())]),
            data: Vec::new(),
        })
    }
    pub const fn bits_per_entry(&self) -> u8 {
        self.bits
    }
    pub fn packed_data(&self) -> &[u64] {
        &self.data
    }
    pub fn palette(&self) -> &[u32] {
        &self.palette
    }
    pub fn get(&self, index: usize) -> Option<u32> {
        if index >= self.kind.len() {
            return None;
        }
        if self.bits == 0 {
            return Some(self.palette[0]);
        }
        let per = 64 / usize::from(self.bits);
        let raw = (self.data[index / per] >> ((index % per) * usize::from(self.bits)))
            & ((1u64 << self.bits) - 1);
        if self.bits == self.global_bits {
            Some(raw as u32)
        } else {
            self.palette.get(raw as usize).copied()
        }
    }
    pub fn set(&mut self, index: usize, value: u32) -> Result<(), PaletteError> {
        if index >= self.kind.len() {
            return Err(PaletteError::Index);
        }
        if u64::from(value) >= (1u64 << self.global_bits) {
            return Err(PaletteError::Value);
        }
        let old = self.get(index).expect("internal palette invariant");
        if old == value {
            return Ok(());
        }
        let old_count = self.counts[&old];
        let new_value = !self.counts.contains_key(&value);
        let remove_old = old_count == 1;
        if new_value || remove_old {
            let mut values: Vec<u32> = (0..self.kind.len())
                .map(|i| self.get(i).expect("internal palette invariant"))
                .collect();
            values[index] = value;
            if remove_old {
                self.counts.remove(&old);
            } else {
                *self.counts.get_mut(&old).unwrap() -= 1;
            }
            *self.counts.entry(value).or_default() += 1;
            self.repack(&values);
        } else {
            *self.counts.get_mut(&old).unwrap() -= 1;
            *self.counts.get_mut(&value).unwrap() += 1;
            let raw = if self.bits == self.global_bits {
                value
            } else {
                self.palette.iter().position(|v| *v == value).unwrap() as u32
            };
            self.write(index, raw);
        }
        Ok(())
    }
    fn repack(&mut self, values: &[u32]) {
        self.palette = self.counts.keys().copied().collect();
        let count = self.palette.len();
        self.bits = if count == 1 {
            0
        } else {
            let bits = (usize::BITS - (count - 1).leading_zeros()) as u8;
            let bits = bits.max(self.kind.min_bits());
            if bits > self.kind.max_bits() {
                self.global_bits
            } else {
                bits
            }
        };
        if self.bits == 0 {
            self.data.clear();
            return;
        }
        let per = 64 / usize::from(self.bits);
        self.data = vec![0; self.kind.len().div_ceil(per)];
        for (i, &value) in values.iter().enumerate() {
            let raw = if self.bits == self.global_bits {
                value
            } else {
                self.palette.binary_search(&value).unwrap() as u32
            };
            self.write(i, raw);
        }
    }
    fn write(&mut self, index: usize, value: u32) {
        let per = 64 / usize::from(self.bits);
        let shift = (index % per) * usize::from(self.bits);
        let mask = ((1u64 << self.bits) - 1) << shift;
        let word = &mut self.data[index / per];
        *word = (*word & !mask) | (u64::from(value) << shift);
    }
}

impl PalettedContainer {
    /// Encode a paletted container in the 1.21.5+ network layout.
    pub fn encode_network(&self) -> Vec<u8> {
        let mut out = vec![self.bits];
        if self.bits == 0 {
            put_varint(&mut out, self.palette[0]);
        } else if self.bits != self.global_bits {
            put_varint(&mut out, self.palette.len() as u32);
            for &value in &self.palette {
                put_varint(&mut out, value);
            }
        }
        for &word in &self.data {
            out.extend_from_slice(&word.to_be_bytes());
        }
        out
    }
}
fn put_varint(out: &mut Vec<u8>, mut value: u32) {
    loop {
        if value & !0x7f == 0 {
            out.push(value as u8);
            break;
        }
        out.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
}
