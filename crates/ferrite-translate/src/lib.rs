//! Multi-version protocol translation in front of the 26.3 (protocol 777) core.
//!
//! The core speaks exactly one protocol ([`ferrite_protocol::PROTOCOL_VERSION`]). Older
//! clients are served by a per-connection [`Translator`]: a chain of version [`Step`]s, one
//! per adjacent version pair (`777 -> 776 -> ...`), sitting between the socket and the
//! core. Every step
//!
//! - remaps packet IDs per state and direction (tables generated at build time from both
//!   versions' `packets.json`, matched by packet name),
//! - rewrites the bodies of packets whose layout changed between the two versions (all
//!   other packets pass through untouched),
//! - remaps block states, items, entity types and every other static registry by name
//!   (tables generated from `blocks.json` / `registries.json`, with per-pair fallback rules
//!   for content the older version lacks),
//! - rewrites chunk sections (palettes and the direct-palette bit width),
//! - filters the synchronized registries and tags sent during configuration, dropping the
//!   entries the older client cannot resolve and remapping every ID that refers to them.
//!
//! Adding an older version means adding one module under [`pairs`] (its rewrites), one
//! `fallbacks.txt`, one entry in `build.rs`, and its vanilla data
//! (`python tools/fetch_vanilla.py <version>`).
//!
//! The network layer drives a [`Translator`] like this:
//!
//! 1. Parse the first (handshake) packet with [`Translator::from_handshake`]. A supported
//!    client yields a translator plus the handshake rewritten for the core; an unsupported
//!    one yields [`Unsupported`], whose [`Unsupported::login_disconnect`] is the vanilla
//!    "Outdated client/server" disconnect (status requests are forwarded untouched, so the
//!    server list shows the version mismatch like vanilla does).
//! 2. Feed every clientbound packet the core produces through [`Translator::clientbound`]
//!    and every packet read from the client through [`Translator::serverbound`], passing
//!    the connection state the packet was encoded in. [`StateTracker`] follows the state
//!    from the core-side serverbound packets if the caller does not track it itself.
//!
//! A translator is not thread-safe by design (`&mut self`): both directions of one
//! connection must go through the same instance, in order.

mod chunk;
mod common;
mod error;
mod nbt;
pub mod pairs;
mod raw;
mod registry_sync;
pub mod tables;
mod tags;
mod translator;

pub use chunk::{ChunkRemap, rewrite_chunk_sections};
pub use error::TranslateError;
pub use raw::RawPacket;
pub use registry_sync::{DynamicRegistries, DynamicRemap};
pub use translator::{
    HandshakeOutcome, ProtocolVersion, StateTracker, Step, Translator, Unsupported, VersionPair,
    is_supported, supported_versions,
};

/// The version the core speaks.
pub const CORE_VERSION: ProtocolVersion = ProtocolVersion {
    protocol: ferrite_protocol::PROTOCOL_VERSION,
    name: ferrite_protocol::GAME_VERSION,
};
