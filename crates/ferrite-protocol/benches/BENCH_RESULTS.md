# Ferrite protocol benchmarks

Command: `cargo bench -p ferrite-protocol` (criterion). Bodies are deterministic
packet-like bytes (structured with some noise); the VarInt mix is periodic one-
through five-byte values. The release profile is the workspace one.

## libdeflate framing (2026-10-28)

"Before" was measured on the original flate2/push-loop implementation; "after" on
the libdeflater/fast-path implementation. Median of 20 samples.

| benchmark | before | after | change |
|---|---:|---:|---:|
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

Frame wins are structural: reusable libdeflate state (no per-packet compressor
allocation), and plain-frame decode copies a slice instead of
`drain().skip().collect()`.

## Compression backend comparison (2026-10-30)

`cargo bench -p ferrite-protocol --bench protocol -- compression_256KiB`
compares the framing backend (libdeflate, level 6) with flate2 driven the way
the old frame code drove it (`ZlibEncoder`/`ZlibDecoder`, level 6, fresh object
per call) on a 256 KiB chunk-shaped payload. Two runs, medians:

| backend | compress | decompress |
|---|---:|---:|
| libdeflate (frame API, reused state) | 4.62 ms (54.1 MiB/s) | 267 µs (935 MiB/s) |
| flate2 (`ZlibEncoder`/`ZlibDecoder`) | 15.83 ms (15.8 MiB/s) | 447 µs (559 MiB/s) |
| ratio | 2.6–3.4x faster | 1.7–2.2x faster |

Run two, on a busier machine, agreed on direction: 5.43 ms vs 14.29 ms
compress, 275 µs vs 598 µs decompress. libdeflate is kept as the default.

flate2 remains the fallback behind the same `FrameEncoder`/`FrameDecoder` API:
`cargo test -p ferrite-protocol --no-default-features` runs the whole frame
suite on the flate2 backend. Both backends reuse their zlib object per
connection and size the decompression buffer from the declared length.

One behavior fix came out of the empty-payload test: a body of length zero now
always uses the `data_length == 0` (uncompressed) marker. Previously a
threshold of 0 compressed it into a zlib stream that the decoder read back as
the packet body, because `data_length == 0` is the marker for "not compressed".

## VarInt fast paths (2026-10-30)

The day-2 branch carried three VarInt/VarLong changes: a windowed decoder (one
32/64-bit load decides the terminator, no branch per byte), a buffered encoder
(fixed stack buffer plus one `extend_from_slice`), and one/two-byte early exits.
They were compared against the unchanged codec two ways.

Same-process A/B, alternating the implementations, best of 40-60 rounds,
`cargo run --release`:

| mix | buffered encode | windowed decode |
|---|---:|---:|
| periodic 1-5 byte | +6% to +24% slower | +19% to +95% slower |
| realistic 70/20/7/3 | +25% to +29% slower | +15% to +24% slower |
| random full-range | −13% (faster) | +6% to +20% slower |

The window and encoder there were harness-local copies, so their absolute
numbers vary with inlining; the sign of the delta was consistent across runs.

Criterion `varint/*` through the crate, median of five separate runs:

| variant | encode_mixed | decode_mixed |
|---|---:|---:|
| unchanged | 10.46 µs (best 9.48) | 10.29 µs (best 10.01) |
| early exits + `#[inline]` | 9.72 µs (best 9.49) | 9.68 µs (best 9.06) |

## Decision

- The windowed decoder and the buffered encoder were **dropped**: they lose on
  every packet-like mix. This matches the 2026-10-28 attempt, where the same two
  changes were reverted after measuring slower.
- The one/two-byte early exits and `#[inline]` on `VarInt::decode` /
  `decode_partial` were **kept**. Decode is ~6% better on medians and ~10% on
  best-of-five; unchanged-code encode moved 7% between the same runs, so the
  decode delta is at the edge of this machine's noise. The change is two
  predictable branches, so it is accepted rather than treated as a proven win.
- VarLong gets no change: it is rare on the wire and the windowed decoder it
  would inherit measured slower.
- Compression stays on libdeflate: it beats flate2 by 2.6-3.4x on compression
  and 1.7-2.2x on decompression for chunk-sized packets. The flate2 path is
  kept buildable as the `--no-default-features` fallback.
- Black-box note: the machine is shared with other agents, and criterion
  comparisons across runs are unusable for small deltas (unchanged frame
  benchmarks once swung 3-8x between consecutive runs). Only same-binary A/Bs
  and best-of-N medians are quoted above.
