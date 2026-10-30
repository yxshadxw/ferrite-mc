# Ferrite protocol-translate: state and what is missing

Status: **not landed.** The work exists only on `origin/wip/day2-2026-10-28`
(commit `0862f98`, "WIP day 2 (unverified)"), and that branch's crate does not
build. Everything below describes that snapshot (17 files, ~1,700 lines plus a
46-line `fallbacks.txt`).

## What the design covers

The crate is meant to sit between the socket and the 26.3/protocol-777 core so
older clients can connect. One `Translator` per connection holds a chain of
adjacent-version `Step`s (`777 -> 776 -> ...`); each step

- remaps packet IDs per state and direction by packet name,
- rewrites the bodies of packets whose layout changed between the two versions,
- remaps blocks, items, entity types and other static registries by name,
- rewrites chunk sections (palettes and direct-palette bit width),
- filters synchronized registries and tags sent during configuration, remapping
  the IDs that pointed at dropped entries.

`src/lib.rs` documents the intended network-layer contract (handshake parsing,
`clientbound`/`serverbound`, `StateTracker`, `Unsupported::disconnect_reason_json`).

## What is present and usable

| file | role | notes |
|---|---|---|
| `build.rs` | generates per-pair tables into `OUT_DIR/<module>.rs`: `NEW_PACKETS`, `OLD_PACKETS`, `PACKETS_DOWN`, `PACKETS_UP`, block-state remaps, `REGISTRIES`, `OLD_STATIC_REGISTRIES`, `OLD_DATA_REGISTRIES`, `FALLBACKS` | 495 lines, reads both versions' `packets.json` / `registries.json` / `blocks.json` |
| `src/tables.rs` | `PairTables`, `RegistryTable`, `Fallback`, `apply_fallback` | lookup side of the generated tables |
| `src/translator.rs` | `Translator`, `Step`, `VersionPair`, `ProtocolVersion`, `StateTracker`, `Unsupported` | reads `crate::pairs::PAIRS` |
| `src/chunk.rs` | `ChunkRemap`, `rewrite_chunk_sections` | has unit tests |
| `src/nbt.rs` | body rewrites that need NBT | has unit tests |
| `src/registry_sync.rs` | `DynamicRegistries`, `DynamicRemap` | filters `registry_data` and rebuilds ID maps |
| `src/tags.rs`, `src/raw.rs`, `src/common.rs`, `src/error.rs` | tag filtering, `RawPacket`, shared helpers, `TranslateError` | no tests |
| `src/pairs/mod.rs` | `PAIRS = &[&v777_v776::PAIR]` | refers to a module that does not exist |
| `src/pairs/v777_v776/fallbacks.txt` | 26.3 content → 26.2 equivalents (concrete/wool slabs, poplar → birch, new items/entities/particles, biome fallbacks) | parsed by `build.rs` |

## Why it was left out

Two hard blockers, plus missing coverage:

1. **The pair module is missing.** `src/pairs/mod.rs` declares
   `pub mod v777_v776;`, but the branch contains only
   `src/pairs/v777_v776/fallbacks.txt` — there is no `mod.rs`. So even with
   data available, the crate cannot compile.
2. **The 26.2 (776) vanilla data is not on this machine.** `build.rs` requires
   `vanilla/26.2/generated/reports/{packets,registries,blocks}.json`; only
   `vanilla/26.3` is present. `cargo check -p ferrite-protocol-translate`
   currently dies in the build script:

   ```
   thread 'main' panicked at crates/ferrite-protocol-translate/build.rs:110:17:
   missing vanilla report .../vanilla/26.2/generated/reports/packets.json
   (os error 3); run `python tools/fetch_vanilla.py 26.2`
   ```

3. **Tests are thin.** Only `chunk.rs` and `nbt.rs` have `#[cfg(test)]`. There is
   no test for the translator chain, packet-ID remapping, registry/tag filtering,
   or an end-to-end login → configuration → play translation, which is what this
   crate needs before it can be trusted.
4. **Nothing calls it.** `ferrite-net` has no translator wiring on that branch;
   the only `ferrite-net` change there is T2 interaction packet routing.

## What is missing

1. **Fetch 26.2 data.** Run `python tools/fetch_vanilla.py 26.2` (Java 25
   `JAVA_HOME`) and keep `vanilla/26.2/generated/` locally; it is gitignored like
   26.3. Without it the generated tables cannot be produced.
2. **Write `src/pairs/v777_v776/mod.rs`.** It has to:
   - `include!(concat!(env!("OUT_DIR"), "/v777_v776.rs"));`
   - build `pub static PAIR: VersionPair` with
     `newer = 26.3/777`, `older = 26.2/776`,
     `tables: &PairTables { new_packets: &NEW_PACKETS, ... , fallbacks: &FALLBACKS }`,
     and `session: || Box::new(Session::default())`.
   - implement `Step` for `Session`: remap IDs with `PACKETS_DOWN` / `PACKETS_UP`,
     pass unchanged packets through as `RawPacket`, and rewrite the bodies of the
     packets whose layout changed. Which packets changed has to be derived from
     the two `packets.json` reports, not guessed.
3. **Decide the 776 body rewrites.** The infrastructure exists
   (`rewrite_chunk_sections`, `DynamicRemap`, `apply_fallback`), but the pair
   module that drives it does not. Expect at least: login/configuration
   registry data and tags (filter + remap), chunk sections (palette width,
   biomes), and any 26.3-only fields added to existing packets.
4. **Tests.** Add round-trip tests per rewrite (encode a 777 packet, translate
   down, decode as 776, assert the expected IDs/values), table tests against the
   generated data (block/registry fallbacks, `-1` handling), and one integration
   test that walks a connection through handshake → login → configuration →
   play with a 776 client.
5. **Wire into `ferrite-net`.** Parse the handshake with
   `Translator::from_handshake`, keep one `Translator` per connection, translate
   clientbound packets after frame decode and serverbound packets before core
   decode, and send `Unsupported::disconnect_reason_json` as the vanilla
   "Outdated client/server" login disconnect. Status requests should stay
   untranslated so the server list shows the version mismatch.
6. **Add the workspace entry.** `crates/*` is globbed, so adding the directory is
   enough; keep `Cargo.lock` in the commit.

## Gotchas

- `PAIRS` is ordered newest first; `supported_versions()` walks it down from
  `CORE_VERSION` and is the single source of truth for the accepted protocol
  range.
- `fallbacks.txt` is parsed by `build.rs` and re-parsed at runtime by
  `tables::apply_fallback`; the two must stay in sync. First matching rule wins;
  unmatched content maps to the registry default in packets and is dropped from
  tags.
- A translator is deliberately `&mut self`: both directions of one connection
  must go through the same instance, in order.
- The `DynamicRemap` IDs are positional indices of the `registry_data` packet,
  not registry names; rewriting them incorrectly breaks chunk biomes and
  dimension types first.
- Keep the crate's `build.rs` dependency on both versions in mind for CI: a
  checkout without `vanilla/26.2` will fail the build, so either commit a small
  fixture of the three reports or make the build script fail with the actionable
  message it already prints.
