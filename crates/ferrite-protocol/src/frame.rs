//! Packet framing: `VarInt length | [VarInt data_length] | body`, with optional zlib compression.
//! Encryption is a byte-stream transform and lives in the network layer below framing.

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use crate::codec::{Decode, DecodeError, Encode, VarInt};

/// Vanilla rejects frames larger than 2 MiB (2^21).
pub const MAX_FRAME_LEN: usize = 1 << 21;
/// Vanilla rejects decompressed bodies larger than 8 MiB (2^23).
pub const MAX_UNCOMPRESSED_LEN: usize = 1 << 23;

#[derive(Default)]
pub struct FrameEncoder {
    /// `None` = compression disabled, `Some(t)` = compress bodies of at least `t` bytes.
    threshold: Option<usize>,
    level: u32,
}

impl FrameEncoder {
    pub fn set_compression(&mut self, threshold: Option<usize>, level: u32) {
        self.threshold = threshold;
        self.level = level;
    }

    /// Frame an already-encoded packet body (ID + fields), appending to `out`.
    pub fn encode(&self, body: &[u8], out: &mut Vec<u8>) {
        let Some(threshold) = self.threshold else {
            VarInt(body.len() as i32).encode(out);
            out.extend_from_slice(body);
            return;
        };
        if body.len() < threshold {
            VarInt(body.len() as i32 + 1).encode(out);
            out.push(0); // data_length = 0: uncompressed
            out.extend_from_slice(body);
            return;
        }
        let mut z = ZlibEncoder::new(
            Vec::with_capacity(body.len() / 2),
            Compression::new(self.level),
        );
        z.write_all(body).expect("write to Vec");
        let compressed = z.finish().expect("finish to Vec");
        let data_len = VarInt(body.len() as i32);
        VarInt((data_len.len() + compressed.len()) as i32).encode(out);
        data_len.encode(out);
        out.extend_from_slice(&compressed);
    }
}

#[derive(Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
    threshold: Option<usize>,
}

impl FrameDecoder {
    pub fn set_compression(&mut self, threshold: Option<usize>) {
        self.threshold = threshold;
    }

    /// Feed raw (already decrypted) bytes from the socket.
    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Pop the next complete packet body (ID + fields), or `None` if more bytes are needed.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, DecodeError> {
        let Some((len, header)) = VarInt::decode_partial(&self.buf)? else {
            return Ok(None);
        };
        let len =
            usize::try_from(len).map_err(|_| DecodeError::Invalid("negative frame length"))?;
        if len > MAX_FRAME_LEN {
            return Err(DecodeError::Invalid("frame too large"));
        }
        if self.buf.len() < header + len {
            return Ok(None);
        }
        let frame: Vec<u8> = self.buf.drain(..header + len).skip(header).collect();
        if self.threshold.is_none() {
            return Ok(Some(frame));
        }
        let mut s = frame.as_slice();
        let data_len = VarInt::decode(&mut s)?.0;
        if data_len == 0 {
            return Ok(Some(s.to_vec()));
        }
        let data_len =
            usize::try_from(data_len).map_err(|_| DecodeError::Invalid("negative data length"))?;
        if data_len > MAX_UNCOMPRESSED_LEN {
            return Err(DecodeError::Invalid("decompressed packet too large"));
        }
        let mut out = Vec::with_capacity(data_len);
        ZlibDecoder::new(s)
            .take(data_len as u64)
            .read_to_end(&mut out)
            .map_err(|_| DecodeError::Invalid("bad zlib data"))?;
        if out.len() != data_len {
            return Err(DecodeError::Invalid("decompressed length mismatch"));
        }
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(threshold: Option<usize>, body: &[u8]) {
        let mut enc = FrameEncoder::default();
        enc.set_compression(threshold, 6);
        let mut dec = FrameDecoder::default();
        dec.set_compression(threshold);
        let mut wire = Vec::new();
        enc.encode(body, &mut wire);
        enc.encode(body, &mut wire);
        // Feed byte-by-byte to exercise partial frames.
        let mut got = Vec::new();
        for b in wire {
            dec.feed(&[b]);
            while let Some(f) = dec.next_frame().unwrap() {
                got.push(f);
            }
        }
        assert_eq!(got, vec![body.to_vec(), body.to_vec()]);
    }

    #[test]
    fn frames_roundtrip() {
        let big = vec![7u8; 4096];
        roundtrip(None, b"\x00hello");
        roundtrip(Some(256), b"\x00hello");
        roundtrip(Some(256), &big);
    }

    #[test]
    fn frame_len_above_max_rejected() {
        let mut dec = FrameDecoder::default();
        let mut wire = Vec::new();
        VarInt(MAX_FRAME_LEN as i32 + 1).encode(&mut wire);
        dec.feed(&wire);
        assert!(matches!(
            dec.next_frame(),
            Err(DecodeError::Invalid("frame too large"))
        ));

        // The limit itself is accepted; only the body is still missing.
        let mut dec = FrameDecoder::default();
        let mut wire = Vec::new();
        VarInt(MAX_FRAME_LEN as i32).encode(&mut wire);
        dec.feed(&wire);
        assert!(dec.next_frame().unwrap().is_none());
    }

    #[test]
    fn compressed_data_len_above_max_rejected() {
        let mut frame = Vec::new();
        VarInt(MAX_UNCOMPRESSED_LEN as i32 + 1).encode(&mut frame);
        frame.extend_from_slice(&[0; 8]);

        let mut dec = FrameDecoder::default();
        dec.set_compression(Some(0));
        let mut wire = Vec::new();
        VarInt(frame.len() as i32).encode(&mut wire);
        wire.extend_from_slice(&frame);
        dec.feed(&wire);
        assert!(matches!(
            dec.next_frame(),
            Err(DecodeError::Invalid("decompressed packet too large"))
        ));
    }

    #[test]
    fn corrupt_zlib_payload_errors() {
        let mut frame = Vec::new();
        VarInt(64).encode(&mut frame);
        // Invalid zlib header (CM nibble is 14, not 8).
        frame.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef, 0x00, 0x11, 0x22, 0x33]);

        let mut dec = FrameDecoder::default();
        dec.set_compression(Some(0));
        let mut wire = Vec::new();
        VarInt(frame.len() as i32).encode(&mut wire);
        wire.extend_from_slice(&frame);
        dec.feed(&wire);
        assert!(matches!(dec.next_frame(), Err(DecodeError::Invalid(_))));
    }
}
