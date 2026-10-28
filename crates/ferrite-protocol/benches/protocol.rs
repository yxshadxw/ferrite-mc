//! Benchmarks for the wire codecs and packet framing.
//!
//! Run with `cargo bench -p ferrite-protocol`. Criterion prints time per
//! operation and throughput (elements/s or bytes/s).

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ferrite_protocol::codec::{Decode, Encode, VarInt};
use ferrite_protocol::frame::{FrameDecoder, FrameEncoder};

/// A mix of one- to five-byte VarInts, skewed the way packet data is:
/// small IDs and lengths dominate, full-range values appear occasionally.
fn mixed_varints() -> Vec<i32> {
    let mut values = Vec::with_capacity(4096);
    let mut state = 0x1234_5678u32;
    for i in 0..4096 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        values.push(match i % 5 {
            0 => (state % 128) as i32,
            1 => (state % 16_384) as i32,
            2 => (state % 2_097_152) as i32,
            3 => (state % 268_435_456) as i32,
            _ => state as i32,
        });
    }
    values
}

/// Deterministic packet-like body: mostly structured bytes with some noise.
fn body(size: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(size);
    let mut state = 0x9e37_79b9u32;
    for i in 0..size {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        out.push(if i % 7 == 0 {
            (state >> 24) as u8
        } else {
            (i % 251) as u8
        });
    }
    out
}

fn varint_benches(c: &mut Criterion) {
    let values = mixed_varints();
    let mut encoded = Vec::new();
    for &v in &values {
        VarInt(v).encode(&mut encoded);
    }

    let mut group = c.benchmark_group("varint");
    group.throughput(Throughput::Elements(values.len() as u64));

    group.bench_function("encode_mixed", |b| {
        let mut out = Vec::with_capacity(encoded.len());
        b.iter(|| {
            out.clear();
            for &v in &values {
                VarInt(v).encode(&mut out);
            }
            black_box(out.len())
        });
    });

    group.bench_function("decode_mixed", |b| {
        b.iter(|| {
            let mut s = encoded.as_slice();
            let mut sum = 0i32;
            while !s.is_empty() {
                sum = sum.wrapping_add(VarInt::decode(&mut s).unwrap().0);
            }
            black_box(sum)
        });
    });

    group.finish();
}

fn frame_benches(c: &mut Criterion) {
    for (label, size) in [("64B", 64usize), ("1KiB", 1024), ("64KiB", 64 * 1024)] {
        let body = body(size);
        let mut group = c.benchmark_group(format!("frame_{label}"));
        group.throughput(Throughput::Bytes(size as u64));

        for (mode, threshold) in [("plain", None), ("compressed", Some(64usize))] {
            let mut enc = FrameEncoder::default();
            enc.set_compression(threshold, 6);
            let mut wire = Vec::new();
            enc.encode(&body, &mut wire);

            group.bench_function(format!("encode_{mode}"), |b| {
                let mut out = Vec::with_capacity(wire.len());
                b.iter(|| {
                    out.clear();
                    enc.encode(black_box(body.as_slice()), &mut out);
                    black_box(out.len())
                });
            });

            group.bench_function(format!("decode_{mode}"), |b| {
                let mut dec = FrameDecoder::default();
                dec.set_compression(threshold);
                b.iter(|| {
                    dec.feed(&wire);
                    black_box(dec.next_frame().unwrap().unwrap())
                });
            });
        }

        group.finish();
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_millis(500))
        .measurement_time(Duration::from_secs(2));
    targets = varint_benches, frame_benches
}
criterion_main!(benches);
