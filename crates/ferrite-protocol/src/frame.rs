//! Packet framing: `VarInt length | [VarInt data_length] | body`, with optional zlib compression.
//! Encryption is a byte-stream transform and lives in the network layer below framing.
//!
//! Compression uses libdeflate through `libdeflater`. The compressor and
//! decompressor states are allocated once per `FrameEncoder`/`FrameDecoder` and
//! reused for every packet; only the output buffer is allocated per frame.

use std::sync::{Mutex, PoisonError};

use libdeflater::{CompressionLvl, Compressor, DecompressionError, Decompressor};

use crate::codec::{Decode, DecodeError, Encode, VarInt};

/// Vanilla rejects frames larger than 2 MiB (2^21).
pub const MAX_FRAME_LEN: usize = 1 << 21;
/// Vanilla rejects decompressed bodies larger than 8 MiB (2^23).
pub const MAX_UNCOMPRESSED_LEN: usize = 1 << 23;

/// Map the configured level onto libdeflate's 1..=12 range. Levels 0 and above
/// 12 (accepted by some callers/flate2 for legacy reasons) are clamped.
fn compression_level(level: u32) -> CompressionLvl {
    CompressionLvl::new(level.clamp(1, 12) as i32).expect("level clamped to libdeflate range")
}

#[derive(Default)]
struct EncoderState {
    level: u32,
    compressor: Option<Compressor>,
    /// Reused output buffer for compressed bytes, grown to the largest bound seen.
    scratch: Vec<u8>,
}

pub struct FrameEncoder {
    /// `None` = compression disabled, `Some(t)` = compress bodies of at least `t` bytes.
    threshold: Option<usize>,
    level: u32,
    // `Mutex` (not `RefCell`) so `&FrameEncoder` stays `Sync`: connection futures
    // that hold one across an await must remain `Send` for `tokio::spawn`.
    state: Mutex<EncoderState>,
}

impl Default for FrameEncoder {
    fn default() -> Self {
        Self {
            threshold: None,
            level: 0,
            state: Mutex::new(EncoderState::default()),
        }
    }
}

impl FrameEncoder {
    pub fn set_compression(&mut self, threshold: Option<usize>, level: u32) {
        self.threshold = threshold;
        self.level = level;
        let state = self.state.get_mut().unwrap_or_else(PoisonError::into_inner);
        if state.level != level {
            state.level = level;
            state.compressor = None;
        }
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
        let mut guard = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let state = &mut *guard;
        let level = state.level;
        let compressor = state
            .compressor
            .get_or_insert_with(|| Compressor::new(compression_level(level)));
        let bound = compressor.zlib_compress_bound(body.len());
        let scratch = &mut state.scratch;
        if scratch.len() < bound {
            scratch.resize(bound, 0);
        }
        let written = compressor
            .zlib_compress(body, &mut scratch[..bound])
            .expect("libdeflate compress bound is sufficient");
        let data_len = VarInt(body.len() as i32);
        VarInt((data_len.len() + written) as i32).encode(out);
        data_len.encode(out);
        out.extend_from_slice(&scratch[..written]);
    }
}

pub struct FrameDecoder {
    buf: Vec<u8>,
    threshold: Option<usize>,
    decompressor: Decompressor,
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self {
            buf: Vec::new(),
            threshold: None,
            decompressor: Decompressor::new(),
        }
    }
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
        let end = header + len;
        if self.threshold.is_none() {
            let frame = self.buf[header..end].to_vec();
            self.buf.drain(..end);
            return Ok(Some(frame));
        }

        // Compressed frame: `[data_length][zlib payload]` inside the frame body.
        let mut body = &self.buf[header..end];
        let data_len = VarInt::decode(&mut body)?.0;
        if data_len == 0 {
            let frame = body.to_vec();
            self.buf.drain(..end);
            return Ok(Some(frame));
        }
        let data_len =
            usize::try_from(data_len).map_err(|_| DecodeError::Invalid("negative data length"))?;
        if data_len > MAX_UNCOMPRESSED_LEN {
            return Err(DecodeError::Invalid("decompressed packet too large"));
        }
        // `libdeflate` writes into a pre-sized slice; `written` must match the
        // declared length exactly (the buffer size enforces the upper bound).
        let mut out = vec![0u8; data_len];
        let written = self
            .decompressor
            .zlib_decompress(body, &mut out)
            .map_err(|e| match e {
                DecompressionError::BadData => DecodeError::Invalid("bad zlib data"),
                DecompressionError::InsufficientSpace => {
                    DecodeError::Invalid("decompressed length mismatch")
                }
            })?;
        if written != data_len {
            return Err(DecodeError::Invalid("decompressed length mismatch"));
        }
        self.buf.drain(..end);
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use flate2::Compression;
    use flate2::read::ZlibDecoder;
    use flate2::write::ZlibEncoder;
    use std::io::{Read, Write};

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

    /// Split a wire buffer into `(declared frame len, frame body)`.
    fn split_frame(wire: &[u8]) -> (usize, &[u8]) {
        let mut s = wire;
        let len = VarInt::decode(&mut s).unwrap().0 as usize;
        assert_eq!(s.len(), len);
        (len, s)
    }

    #[test]
    fn frames_compressed_by_libdeflate_decode_with_flate2() {
        let body: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
        let mut enc = FrameEncoder::default();
        enc.set_compression(Some(0), 9);
        let mut wire = Vec::new();
        enc.encode(&body, &mut wire);

        let (_, frame) = split_frame(&wire);
        let mut s = frame;
        let data_len = VarInt::decode(&mut s).unwrap().0 as usize;
        assert_eq!(data_len, body.len());

        let mut decoded = Vec::new();
        ZlibDecoder::new(s).read_to_end(&mut decoded).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn frames_compressed_by_flate2_decode_with_libdeflate() {
        let body: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
        let mut z = ZlibEncoder::new(Vec::new(), Compression::new(9));
        z.write_all(&body).unwrap();
        let compressed = z.finish().unwrap();

        let mut frame = Vec::new();
        VarInt(body.len() as i32).encode(&mut frame);
        frame.extend_from_slice(&compressed);
        let mut wire = Vec::new();
        VarInt(frame.len() as i32).encode(&mut wire);
        wire.extend_from_slice(&frame);

        let mut dec = FrameDecoder::default();
        dec.set_compression(Some(0));
        dec.feed(&wire);
        assert_eq!(dec.next_frame().unwrap().unwrap(), body);
    }

    #[test]
    fn compressed_declared_length_mismatch_rejected() {
        let body = b"hello, ferrite";
        let mut z = ZlibEncoder::new(Vec::new(), Compression::new(6));
        z.write_all(body).unwrap();
        let compressed = z.finish().unwrap();

        for declared in [body.len() as i32 - 1, body.len() as i32 + 1] {
            let mut frame = Vec::new();
            VarInt(declared).encode(&mut frame);
            frame.extend_from_slice(&compressed);
            let mut wire = Vec::new();
            VarInt(frame.len() as i32).encode(&mut wire);
            wire.extend_from_slice(&frame);

            let mut dec = FrameDecoder::default();
            dec.set_compression(Some(0));
            dec.feed(&wire);
            assert!(matches!(
                dec.next_frame(),
                Err(DecodeError::Invalid("decompressed length mismatch"))
            ));
        }
    }
}
