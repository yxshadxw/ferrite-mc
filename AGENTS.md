# Ferrite — agent guide

Ferrite is a Minecraft Java server written in Rust: Folia-style regionized multithreading,
plus (later) Bukkit/Paper plugin compatibility through an embedded JVM.

- Target: **Minecraft Java 26.3, protocol 777** (`ferrite_protocol::PROTOCOL_VERSION`).
- Never commit Mojang code or assets. Vanilla data is extracted locally from the official
  server jar into `vanilla/` (gitignored).

## Build

Rust is in `~/.cargo/bin` (not on PATH by default). The toolchain is pinned to
`stable-x86_64-pc-windows-msvc` by `rust-toolchain.toml`.

```sh
source tools/env.sh          # Git Bash      (PowerShell: . .\tools\env.ps1)
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

## Vanilla data

`python tools/fetch_vanilla.py` downloads the 26.3 server jar and runs its data generator
into `vanilla/26.3/generated/` (`reports/packets.json`, `reports/blocks.json`,
`reports/registries.json`, `data/minecraft/**`). It needs Java 25:
`JAVA_HOME="C:/Program Files/Eclipse Adoptium/jdk-25.0.5.8-hotspot"`.
This is already done on the dev machine; read the files instead of guessing IDs.

## Layout

| Crate | Owns |
|---|---|
| `ferrite-nbt` | NBT model, binary IO, SNBT |
| `ferrite-protocol` | `Encode`/`Decode`, `VarInt`, framing (`frame.rs`), packet structs under `packets/<state>.rs` |
| `ferrite-net` | tokio connections, encryption, login/configuration/play bootstrap |
| `ferrite-region` | Folia-style regionizer and region schedulers |
| `ferrite-world` | chunks, paletted containers, Anvil IO, lighting |
| `ferrite-data` | vanilla data extraction and generated tables |
| `ferrite-server` | binary: config, console, wiring |

## Conventions

- Edition 2024. Keep `cargo clippy -D warnings` and `cargo fmt` clean.
- Add dependencies in the crate's own `Cargo.toml` (prefer `workspace = true` for ones
  already in the root `[workspace.dependencies]`); do not reorganize the root manifest.
- Packets implement `Encode + Decode + Packet` by hand (see `packets/handshake.rs`).
  Packet IDs must match the vanilla `packets.json` report for 26.3.
- Every codec gets a round-trip unit test; bug fixes get a regression test.
- Do not edit crates outside the scope of your task; ask instead.
