# Ferrite protocol benchmarks — libdeflate framing + VarInt fast paths

Command: `cargo bench -p ferrite-protocol` (criterion, release profile, median of
20 samples). Bodies are deterministic packet-like bytes (structured with some
noise); the VarInt mix is periodic one- through five-byte values.

"Before" was measured on the original flate2/push-loop implementation before any
edits; "after" on the libdeflater/fast-path implementation.

| benchmark | before | after | change |
|---|---:|---:|---:|
| varint/encode_mixed | 6.00 µs | 8.40 µs | +40.0% |
| varint/decode_mixed | 6.89 µs | 8.24 µs | +19.5% |
| frame_64B/encode_plain | 7.38 ns | 4.81 ns | −34.8% |
| frame_64B/decode_plain | 71.6 ns | 27.4 ns | −61.8% |
| frame_64B/encode_compressed | 20.53 µs | 4.51 µs | −78.0% |
| frame_64B/decode_compressed | 3.266 µs | 177.4 ns | −94.6% |
| frame_1KiB/encode_plain | 13.30 ns | 9.28 ns | −30.2% |
| frame_1KiB/decode_plain | 526.2 ns | 48.3 ns | −90.8% |
| frame_1KiB/encode_compressed | 26.44 µs | 9.50 µs | −64.1% |
| frame_1KiB/decode_compressed | 7.563 µs | 2.670 µs | −64.7% |
| frame_64KiB/encode_plain | 1.006 µs | 0.976 µs | −3.0% |
| frame_64KiB/decode_plain | 40.03 µs | 2.181 µs | −94.6% |
| frame_64KiB/encode_compressed | 2.213 ms | 752.6 µs | −66.0% |
| frame_64KiB/decode_compressed | 147.75 µs | 55.28 µs | −62.6% |

## Caveats

- Benchmarks ran on a busy multi-agent machine; cross-run numbers are noisy.
  Consecutive runs of the unchanged VarInt encoder varied between 7.3 µs and
  10.9 µs, so the VarInt deltas above are indicative, not precise.
- Frame wins are structural: reusable libdeflate state (no per-packet
  compressor allocation), and plain-frame decode now copies a slice instead of
  `drain().skip().collect()`.
- A same-process A/B of the old and new VarInt implementations showed the
  encode shape roughly at parity or faster, and decode about 5–20% slower on
  this periodic, perfectly predictable mix; on a realistic random mix (70%
  one-byte values) the gap was ~5%. The five-byte branchless window pays off
  only when continuation bits are hard to predict; on predictable data the
  old early-exit loop is hard to beat.

## Decision

The VarInt/VarLong fast paths were measured slower on predictable and realistic mixes,
so they were reverted (`codec.rs` is unchanged). Only the libdeflate framing and the
criterion benches are kept; the two `varint/*` rows above describe the reverted variant.
