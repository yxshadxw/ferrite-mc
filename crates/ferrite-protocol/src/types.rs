//! Composite protocol types shared across packets.

use crate::codec::{Decode, DecodeError, Encode};

/// Block position packed as `x:26 | z:26 | y:12` in a big-endian i64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn pack(self) -> i64 {
        ((self.x as i64 & 0x3ff_ffff) << 38)
            | ((self.z as i64 & 0x3ff_ffff) << 12)
            | (self.y as i64 & 0xfff)
    }

    pub fn unpack(v: i64) -> Self {
        Self {
            x: (v >> 38) as i32,
            y: (v << 52 >> 52) as i32,
            z: (v << 26 >> 38) as i32,
        }
    }
}

impl Encode for BlockPos {
    fn encode(&self, out: &mut Vec<u8>) {
        self.pack().encode(out)
    }
}
impl Decode for BlockPos {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self::unpack(i64::decode(buf)?))
    }
}

/// Namespaced identifier such as `minecraft:overworld`. Stored in full form.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident(pub String);

impl Ident {
    pub fn new(s: impl Into<String>) -> Self {
        let s = s.into();
        if s.contains(':') {
            Self(s)
        } else {
            Self(format!("minecraft:{s}"))
        }
    }
}

impl Encode for Ident {
    fn encode(&self, out: &mut Vec<u8>) {
        self.0.encode(out)
    }
}
impl Decode for Ident {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Self::new(crate::codec::decode_string(buf, 32767)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_pos_packs() {
        for p in [
            BlockPos::new(0, 0, 0),
            BlockPos::new(-1, -64, 1),
            BlockPos::new(30_000_000, 319, -30_000_000),
        ] {
            assert_eq!(BlockPos::unpack(p.pack()), p);
        }
        // Known value from the protocol docs: (18357644, 831, -20882616).
        let p = BlockPos::new(18357644, 831, -20882616);
        assert_eq!(
            p.pack() as u64,
            0b01000110000001110110001100_10110000010101101101001000_001100111111
        );
    }
}
