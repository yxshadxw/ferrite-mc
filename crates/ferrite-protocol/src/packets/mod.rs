//! Packet definitions, one module per connection state.
//!
//! Packet IDs come from the vanilla data generator report (`packets.json`) for
//! [`crate::GAME_VERSION`]; see `crates/ferrite-data`.

pub mod handshake;
