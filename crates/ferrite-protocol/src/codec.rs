use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("unexpected end of input")]
    Eof,
    #[error("VarInt too long")]
    VarIntTooLong,
    #[error("string length {0} exceeds max {1}")]
    StringTooLong(usize, usize),
    #[error("invalid UTF-8")]
    InvalidUtf8,
    #[error("invalid value: {0}")]
    Invalid(&'static str),
    #[error("nbt: {0}")]
    Nbt(String),
}

/// Writes a value in protocol form. Encoding into a `Vec` cannot fail.
pub trait Encode {
    fn encode(&self, out: &mut Vec<u8>);
}

/// Reads a value in protocol form, advancing `buf` past it.
pub trait Decode: Sized {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError>;
}

pub fn take<'a>(buf: &mut &'a [u8], n: usize) -> Result<&'a [u8], DecodeError> {
    if buf.len() < n {
        return Err(DecodeError::Eof);
    }
    let (head, tail) = buf.split_at(n);
    *buf = tail;
    Ok(head)
}

macro_rules! impl_be {
    ($($t:ty),*) => {$(
        impl Encode for $t {
            fn encode(&self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_be_bytes()); }
        }
        impl Decode for $t {
            fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
                let b = take(buf, size_of::<$t>())?;
                Ok(<$t>::from_be_bytes(b.try_into().unwrap()))
            }
        }
    )*};
}
impl_be!(u8, i8, u16, i16, u32, i32, u64, i64, u128, f32, f64);

impl Encode for bool {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(*self as u8);
    }
}
impl Decode for bool {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        match u8::decode(buf)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::Invalid("bool")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VarInt(pub i32);

impl VarInt {
    pub const MAX_LEN: usize = 5;

    /// Encoded length in bytes (1..=5).
    #[allow(clippy::len_without_is_empty)]
    pub fn len(self) -> usize {
        match self.0 as u32 {
            0..=0x7f => 1,
            0x80..=0x3fff => 2,
            0x4000..=0x1f_ffff => 3,
            0x20_0000..=0xfff_ffff => 4,
            _ => 5,
        }
    }

    /// Decode from a partial buffer: `Ok(None)` means more bytes are needed.
    pub fn decode_partial(buf: &[u8]) -> Result<Option<(i32, usize)>, DecodeError> {
        let mut value = 0u32;
        for i in 0..Self::MAX_LEN {
            let Some(&b) = buf.get(i) else {
                return Ok(None);
            };
            value |= ((b & 0x7f) as u32) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(Some((value as i32, i + 1)));
            }
        }
        Err(DecodeError::VarIntTooLong)
    }
}

impl Encode for VarInt {
    fn encode(&self, out: &mut Vec<u8>) {
        let mut v = self.0 as u32;
        loop {
            if v & !0x7f == 0 {
                out.push(v as u8);
                return;
            }
            out.push((v as u8 & 0x7f) | 0x80);
            v >>= 7;
        }
    }
}
impl Decode for VarInt {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        match Self::decode_partial(buf)? {
            Some((v, n)) => {
                *buf = &buf[n..];
                Ok(VarInt(v))
            }
            None => Err(DecodeError::Eof),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VarLong(pub i64);

impl Encode for VarLong {
    fn encode(&self, out: &mut Vec<u8>) {
        let mut v = self.0 as u64;
        loop {
            if v & !0x7f == 0 {
                out.push(v as u8);
                return;
            }
            out.push((v as u8 & 0x7f) | 0x80);
            v >>= 7;
        }
    }
}
impl Decode for VarLong {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let mut value = 0u64;
        for i in 0..10 {
            let b = u8::decode(buf)?;
            value |= ((b & 0x7f) as u64) << (7 * i);
            if b & 0x80 == 0 {
                return Ok(VarLong(value as i64));
            }
        }
        Err(DecodeError::VarIntTooLong)
    }
}

/// Default max string length in characters.
pub const MAX_STRING: usize = 32767;

impl Encode for str {
    fn encode(&self, out: &mut Vec<u8>) {
        VarInt(self.len() as i32).encode(out);
        out.extend_from_slice(self.as_bytes());
    }
}
impl Encode for String {
    fn encode(&self, out: &mut Vec<u8>) {
        self.as_str().encode(out)
    }
}
impl Decode for String {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        decode_string(buf, MAX_STRING)
    }
}

pub fn decode_string(buf: &mut &[u8], max_chars: usize) -> Result<String, DecodeError> {
    let len = VarInt::decode(buf)?.0;
    let len = usize::try_from(len).map_err(|_| DecodeError::Invalid("negative string length"))?;
    if len > max_chars * 3 {
        return Err(DecodeError::StringTooLong(len, max_chars * 3));
    }
    let s = std::str::from_utf8(take(buf, len)?).map_err(|_| DecodeError::InvalidUtf8)?;
    let chars = s.chars().count();
    if chars > max_chars {
        return Err(DecodeError::StringTooLong(chars, max_chars));
    }
    Ok(s.to_owned())
}

impl Encode for Uuid {
    fn encode(&self, out: &mut Vec<u8>) {
        self.as_u128().encode(out)
    }
}
impl Decode for Uuid {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(Uuid::from_u128(u128::decode(buf)?))
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Some(v) => {
                true.encode(out);
                v.encode(out);
            }
            None => false.encode(out),
        }
    }
}
impl<T: Decode> Decode for Option<T> {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        if bool::decode(buf)? {
            Ok(Some(T::decode(buf)?))
        } else {
            Ok(None)
        }
    }
}

/// Length-prefixed (VarInt) array.
impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, out: &mut Vec<u8>) {
        VarInt(self.len() as i32).encode(out);
        for v in self {
            v.encode(out);
        }
    }
}
impl<T: Decode> Decode for Vec<T> {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let len = VarInt::decode(buf)?.0;
        let len =
            usize::try_from(len).map_err(|_| DecodeError::Invalid("negative array length"))?;
        // Every element is at least one byte, so cap preallocation by remaining input.
        let mut v = Vec::with_capacity(len.min(buf.len()));
        for _ in 0..len {
            v.push(T::decode(buf)?);
        }
        Ok(v)
    }
}

impl<T: Encode + ?Sized> Encode for &T {
    fn encode(&self, out: &mut Vec<u8>) {
        (**self).encode(out)
    }
}

/// Raw bytes that consume the rest of the packet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestBytes(pub Vec<u8>);

impl Encode for RestBytes {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0)
    }
}
impl Decode for RestBytes {
    fn decode(buf: &mut &[u8]) -> Result<Self, DecodeError> {
        let v = buf.to_vec();
        *buf = &[];
        Ok(RestBytes(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T: Encode + Decode + PartialEq + std::fmt::Debug>(v: T) -> Vec<u8> {
        let mut out = Vec::new();
        v.encode(&mut out);
        let mut s = out.as_slice();
        assert_eq!(T::decode(&mut s).unwrap(), v);
        assert!(s.is_empty());
        out
    }

    #[test]
    fn varint_known_values() {
        assert_eq!(roundtrip(VarInt(0)), [0x00]);
        assert_eq!(roundtrip(VarInt(127)), [0x7f]);
        assert_eq!(roundtrip(VarInt(128)), [0x80, 0x01]);
        assert_eq!(roundtrip(VarInt(25565)), [0xdd, 0xc7, 0x01]);
        assert_eq!(roundtrip(VarInt(i32::MAX)), [0xff, 0xff, 0xff, 0xff, 0x07]);
        assert_eq!(roundtrip(VarInt(-1)), [0xff, 0xff, 0xff, 0xff, 0x0f]);
        for v in [0, 1, 300, -300, i32::MIN, i32::MAX] {
            assert_eq!(VarInt(v).len(), roundtrip(VarInt(v)).len());
        }
    }

    #[test]
    fn varlong_known_values() {
        let mut expected = vec![0xff; 9];
        expected.push(0x01);
        assert_eq!(roundtrip(VarLong(-1)), expected);
        roundtrip(VarLong(i64::MIN));
    }

    #[test]
    fn strings_options_arrays_uuids() {
        roundtrip("héllo".to_string());
        roundtrip(Some(5i32));
        roundtrip(None::<i32>);
        roundtrip(vec![1u8, 2, 3]);
        roundtrip(Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef));
    }

    #[test]
    fn partial_varint() {
        assert_eq!(VarInt::decode_partial(&[0x80]).unwrap(), None);
        assert!(VarInt::decode_partial(&[0x80; 5]).is_err());
    }
}
