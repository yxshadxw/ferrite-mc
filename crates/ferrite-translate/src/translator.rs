use ferrite_protocol::packets::handshake::{Intent, Intention};
use ferrite_protocol::{Bound, Decode, Encode, State};

use crate::pairs::PAIRS;
use crate::{CORE_VERSION, RawPacket, TranslateError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolVersion {
    pub protocol: i32,
    pub name: &'static str,
}

/// The per-connection half of one version pair: translates between the pair's newer
/// version (towards the core) and its older version (towards the client).
///
/// Both methods take the packet in their input version and push zero or more packets in
/// their output version to `out`. `state` is the connection state the packet belongs to.
pub trait Step: Send {
    /// Newer -> older.
    fn clientbound(
        &mut self,
        state: State,
        packet: RawPacket,
        out: &mut Vec<RawPacket>,
    ) -> Result<(), TranslateError>;
    /// Older -> newer.
    fn serverbound(
        &mut self,
        state: State,
        packet: RawPacket,
        out: &mut Vec<RawPacket>,
    ) -> Result<(), TranslateError>;
}

/// One adjacent version step, e.g. 26.3 (777) -> 26.2 (776).
pub struct VersionPair {
    pub newer: ProtocolVersion,
    pub older: ProtocolVersion,
    pub tables: &'static crate::tables::PairTables,
    /// A fresh per-connection session.
    pub session: fn() -> Box<dyn Step>,
}

/// Every version a client may connect with, newest (the core's) first.
pub fn supported_versions() -> Vec<ProtocolVersion> {
    let mut out = vec![CORE_VERSION];
    let mut current = CORE_VERSION.protocol;
    while let Some(pair) = PAIRS.iter().find(|p| p.newer.protocol == current) {
        out.push(pair.older);
        current = pair.older.protocol;
    }
    out
}

pub fn is_supported(protocol: i32) -> bool {
    supported_versions().iter().any(|v| v.protocol == protocol)
}

/// Translation for one connection: the chain of steps from the core down to the client's
/// version (empty for clients on the core's version).
pub struct Translator {
    client: ProtocolVersion,
    /// Ordered from the core side to the client side.
    steps: Vec<Box<dyn Step>>,
}

impl std::fmt::Debug for Translator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Translator")
            .field("client", &self.client)
            .field("steps", &self.steps.len())
            .finish()
    }
}

/// A client protocol no chain reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported {
    pub client_protocol: i32,
}

impl Unsupported {
    /// Vanilla's `multiplayer.disconnect.outdated_client` / `outdated_server`, naming the
    /// supported version range, as a JSON text component.
    pub fn disconnect_reason_json(&self) -> String {
        let versions = supported_versions();
        let range = match (versions.last(), versions.first()) {
            (Some(oldest), Some(newest)) if oldest != newest => {
                format!("{}-{}", oldest.name, newest.name)
            }
            _ => CORE_VERSION.name.to_owned(),
        };
        let key = if self.client_protocol < CORE_VERSION.protocol {
            "multiplayer.disconnect.outdated_client"
        } else {
            "multiplayer.disconnect.outdated_server"
        };
        serde_json::json!({ "translate": key, "with": [range] }).to_string()
    }

    /// The `login` state disconnect (ID 0, JSON text) to send before closing. Its layout
    /// has been stable for every version, so it is valid for the unsupported client too.
    pub fn login_disconnect(&self) -> RawPacket {
        let mut body = Vec::new();
        self.disconnect_reason_json().encode(&mut body);
        RawPacket::new(0, body)
    }
}

/// Result of [`Translator::from_handshake`].
#[derive(Debug)]
pub enum HandshakeOutcome {
    Supported {
        translator: Translator,
        /// The handshake as the core expects it (core protocol version).
        forwarded: RawPacket,
        intention: Intention,
    },
    /// Status requests should be forwarded untouched (the client then shows the version
    /// mismatch); logins answered with [`Unsupported::login_disconnect`].
    Unsupported {
        unsupported: Unsupported,
        intention: Intention,
    },
}

impl Translator {
    pub fn new(client_protocol: i32) -> Result<Self, Unsupported> {
        let mut steps = Vec::new();
        let mut current = CORE_VERSION;
        while current.protocol != client_protocol {
            let Some(pair) = PAIRS.iter().find(|p| p.newer.protocol == current.protocol) else {
                return Err(Unsupported { client_protocol });
            };
            steps.push((pair.session)());
            current = pair.older;
        }
        Ok(Self {
            client: current,
            steps,
        })
    }

    /// Parse the client's handshake (in any version: its layout is version independent)
    /// and set up translation for it.
    pub fn from_handshake(packet: &RawPacket) -> Result<HandshakeOutcome, TranslateError> {
        if packet.id != 0 {
            return Err(TranslateError::Invalid("expected handshake"));
        }
        let mut buf = packet.body.as_slice();
        let intention = Intention::decode(&mut buf)?;
        match Self::new(intention.protocol_version) {
            Err(unsupported) => Ok(HandshakeOutcome::Unsupported {
                unsupported,
                intention,
            }),
            Ok(mut translator) => {
                let mut out = translator.serverbound(State::Handshaking, packet.clone())?;
                let forwarded = out
                    .pop()
                    .filter(|_| out.is_empty())
                    .ok_or(TranslateError::Invalid("handshake translated to no packet"))?;
                let core_intention = Intention::decode(&mut forwarded.body.as_slice())?;
                debug_assert_eq!(core_intention.protocol_version, CORE_VERSION.protocol);
                Ok(HandshakeOutcome::Supported {
                    translator,
                    forwarded,
                    intention,
                })
            }
        }
    }

    pub fn client_version(&self) -> ProtocolVersion {
        self.client
    }

    /// `true` when the client speaks the core's protocol and packets pass untouched.
    pub fn is_passthrough(&self) -> bool {
        self.steps.is_empty()
    }

    /// Core packet -> packets for the client.
    pub fn clientbound(
        &mut self,
        state: State,
        packet: RawPacket,
    ) -> Result<Vec<RawPacket>, TranslateError> {
        let mut packets = vec![packet];
        for step in &mut self.steps {
            let mut next = Vec::with_capacity(packets.len());
            for packet in packets {
                step.clientbound(state, packet, &mut next)?;
            }
            packets = next;
        }
        Ok(packets)
    }

    /// Client packet -> packets for the core.
    pub fn serverbound(
        &mut self,
        state: State,
        packet: RawPacket,
    ) -> Result<Vec<RawPacket>, TranslateError> {
        let mut packets = vec![packet];
        for step in self.steps.iter_mut().rev() {
            let mut next = Vec::with_capacity(packets.len());
            for packet in packets {
                step.serverbound(state, packet, &mut next)?;
            }
            packets = next;
        }
        Ok(packets)
    }
}

/// Follows the connection state from the serverbound packets as the core receives them
/// (core protocol IDs, i.e. after [`Translator::serverbound`]). The switch happens when
/// the core reads the packet, like vanilla: the client's handshake intent, the login and
/// configuration acknowledgements, and `configuration_acknowledged` in play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateTracker {
    pub state: State,
}

impl Default for StateTracker {
    fn default() -> Self {
        Self {
            state: State::Handshaking,
        }
    }
}

impl StateTracker {
    /// Update the state after the core has received `packet` in `self.state`.
    pub fn on_serverbound(&mut self, packet: &RawPacket) {
        let core = core_tables();
        let name = core
            .and_then(|t| t.new_packet_name(self.state, Bound::Serverbound, packet.id))
            .unwrap_or_default();
        self.state = match (self.state, name) {
            (State::Handshaking, _) if packet.id == 0 => {
                match Intention::decode(&mut packet.body.as_slice()).map(|i| i.intent) {
                    Ok(Intent::Status) => State::Status,
                    Ok(Intent::Login | Intent::Transfer) => State::Login,
                    Err(_) => State::Handshaking,
                }
            }
            (State::Login, "minecraft:login_acknowledged") => State::Configuration,
            (State::Configuration, "minecraft:finish_configuration") => State::Play,
            (State::Play, "minecraft:configuration_acknowledged") => State::Configuration,
            (state, _) => state,
        };
    }
}

/// Packet names of the core version (the newer side of the first pair).
fn core_tables() -> Option<&'static crate::tables::PairTables> {
    PAIRS
        .iter()
        .find(|p| p.newer.protocol == CORE_VERSION.protocol)
        .map(|p| p.tables)
}
