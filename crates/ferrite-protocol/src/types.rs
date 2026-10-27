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
    #[allow(clippy::unusual_byte_groupings)] // grouped as x / z / y fields
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

/// A plain-text network NBT component encoded as a nameless TAG_String.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainText(pub String);
impl Encode for PlainText {
    fn encode(&self, out: &mut Vec<u8>) {
        let mut bytes = Vec::new();
        for c in self.0.encode_utf16() {
            if (1..=0x7f).contains(&c) {
                bytes.push(c as u8);
            } else if c <= 0x7ff {
                bytes.push((0xc0 | (c >> 6)) as u8);
                bytes.push((0x80 | (c & 0x3f)) as u8);
            } else {
                bytes.push((0xe0 | (c >> 12)) as u8);
                bytes.push((0x80 | ((c >> 6) & 0x3f)) as u8);
                bytes.push((0x80 | (c & 0x3f)) as u8);
            }
        }
        8u8.encode(out);
        u16::try_from(bytes.len())
            .expect("NBT string exceeds 65535 bytes")
            .encode(out);
        out.extend_from_slice(&bytes);
    }
}
impl Decode for PlainText {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        if u8::decode(buf)? != 8 {
            return Err(DecodeError::Invalid("expected NBT string"));
        }
        let n = u16::decode(buf)? as usize;
        let b = crate::codec::take(buf, n)?;
        let mut units = Vec::new();
        let mut i = 0;
        while i < b.len() {
            let x = b[i];
            let (value, width) = if (1..=0x7f).contains(&x) {
                (x as u16, 1)
            } else if x & 0xe0 == 0xc0 && i + 1 < b.len() && b[i + 1] & 0xc0 == 0x80 {
                ((((x & 31) as u16) << 6) | ((b[i + 1] & 63) as u16), 2)
            } else if x & 0xf0 == 0xe0
                && i + 2 < b.len()
                && b[i + 1] & 0xc0 == 0x80
                && b[i + 2] & 0xc0 == 0x80
            {
                (
                    (((x & 15) as u16) << 12)
                        | (((b[i + 1] & 63) as u16) << 6)
                        | ((b[i + 2] & 63) as u16),
                    3,
                )
            } else {
                return Err(DecodeError::Invalid("invalid modified UTF-8"));
            };
            units.push(value);
            i += width;
        }
        Ok(Self(String::from_utf16(&units).map_err(|_| {
            DecodeError::Invalid("invalid modified UTF-8")
        })?))
    }
}
