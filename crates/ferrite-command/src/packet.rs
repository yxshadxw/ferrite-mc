//! Serialization of a dispatcher to the clientbound `Commands` packet body.
//!
//! Layout (protocol 777): `Prefixed Array of Node`, then the root index (VarInt). Node:
//! flags byte (`0x03` type: 0 root / 1 literal / 2 argument, `0x04` executable,
//! `0x08` has redirect, `0x10` has suggestion type, `0x20` restricted), children
//! (VarInt array), redirect (VarInt, if `0x08`), name (String, literal/argument),
//! parser id (VarInt) + properties (argument), suggestion type (Identifier, if `0x10`).

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::Path;

use ferrite_protocol::{Encode, VarInt};

use crate::arguments::ArgumentType;
use crate::dispatcher::CommandDispatcher;
use crate::tree::{NodeId, NodeKind, SuggestionKind};

pub const NODE_TYPE_ROOT: u8 = 0;
pub const NODE_TYPE_LITERAL: u8 = 1;
pub const NODE_TYPE_ARGUMENT: u8 = 2;
pub const FLAG_TYPE_MASK: u8 = 0x03;
pub const FLAG_EXECUTABLE: u8 = 0x04;
pub const FLAG_REDIRECT: u8 = 0x08;
pub const FLAG_SUGGESTIONS: u8 = 0x10;
pub const FLAG_RESTRICTED: u8 = 0x20;

/// Parser identifier → protocol id, from the `minecraft:command_argument_type` registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserRegistry {
    ids: HashMap<String, i32>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("registries.json has no usable `minecraft:command_argument_type` entries")]
    Missing,
}

impl ParserRegistry {
    /// Ids assigned in iteration order (0, 1, 2, ...).
    pub fn from_ordered<I: IntoIterator<Item = T>, T: Into<String>>(ids: I) -> Self {
        Self {
            ids: ids
                .into_iter()
                .zip(0..)
                .map(|(id, n)| (id.into(), n))
                .collect(),
        }
    }

    /// Reads `entries.*.protocol_id` of `minecraft:command_argument_type` from a vanilla
    /// `reports/registries.json`.
    pub fn from_registries_json(json: &str) -> Result<Self, RegistryError> {
        let value: serde_json::Value = serde_json::from_str(json)?;
        let entries = value
            .get("minecraft:command_argument_type")
            .and_then(|r| r.get("entries"))
            .and_then(|e| e.as_object())
            .ok_or(RegistryError::Missing)?;
        let mut ids = HashMap::new();
        for (name, entry) in entries {
            let id = entry
                .get("protocol_id")
                .and_then(|v| v.as_i64())
                .and_then(|v| i32::try_from(v).ok())
                .ok_or(RegistryError::Missing)?;
            ids.insert(name.clone(), id);
        }
        if ids.is_empty() {
            return Err(RegistryError::Missing);
        }
        Ok(Self { ids })
    }

    /// Loads `<reports_dir>/registries.json`.
    pub fn load(reports_dir: impl AsRef<Path>) -> Result<Self, RegistryError> {
        let json = std::fs::read_to_string(reports_dir.as_ref().join("registries.json"))?;
        Self::from_registries_json(&json)
    }

    pub fn id(&self, parser: &str) -> Option<i32> {
        self.ids.get(parser).copied()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Identifiers ordered by protocol id.
    pub fn identifiers(&self) -> Vec<&str> {
        let mut v: Vec<(&str, i32)> = self.ids.iter().map(|(k, &v)| (k.as_str(), v)).collect();
        v.sort_by_key(|&(_, id)| id);
        v.into_iter().map(|(k, _)| k).collect()
    }
}

/// A serialized `Commands` packet body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializedCommands {
    /// Packet body without the packet id.
    pub body: Vec<u8>,
    pub node_count: usize,
    /// Parser identifiers missing from the registry. Their nodes were sent as
    /// `brigadier:string` (single word) with `minecraft:ask_server` suggestions so the
    /// client can still decode the packet.
    pub unknown_parsers: BTreeSet<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SerializeError {
    /// An unknown parser needs the `brigadier:string` fallback, which the registry lacks.
    #[error("parser registry has no `{0}`")]
    MissingParser(String),
}

impl<S> CommandDispatcher<S> {
    /// Encodes the part of the graph `source` can use: nodes whose requirement fails are
    /// omitted with their subtrees, and redirects to omitted nodes are dropped (as in
    /// vanilla's `Commands.sendCommands`). Nodes are numbered breadth-first, root first.
    pub fn serialize_commands(
        &self,
        source: &S,
        registry: &ParserRegistry,
    ) -> Result<SerializedCommands, SerializeError> {
        let mut order = vec![self.root()];
        let mut index = HashMap::from([(self.root(), 0usize)]);
        let mut visible_children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let mut queue = VecDeque::from([self.root()]);
        while let Some(id) = queue.pop_front() {
            let children: Vec<NodeId> = self
                .node(id)
                .children()
                .iter()
                .copied()
                .filter(|&c| self.node(c).can_use(source))
                .collect();
            for &child in &children {
                if let std::collections::hash_map::Entry::Vacant(slot) = index.entry(child) {
                    slot.insert(order.len());
                    order.push(child);
                    queue.push_back(child);
                }
            }
            visible_children.insert(id, children);
        }

        let fallback = ArgumentType::word();
        let mut unknown_parsers = BTreeSet::new();
        let mut body = Vec::new();
        VarInt(order.len() as i32).encode(&mut body);
        for &id in &order {
            let node = self.node(id);
            let redirect = node.redirect().and_then(|t| index.get(&t).copied());
            let mut flags = match node.kind() {
                NodeKind::Root => NODE_TYPE_ROOT,
                NodeKind::Literal(_) => NODE_TYPE_LITERAL,
                NodeKind::Argument { .. } => NODE_TYPE_ARGUMENT,
            };
            if node.is_executable() {
                flags |= FLAG_EXECUTABLE;
            }
            if redirect.is_some() {
                flags |= FLAG_REDIRECT;
            }
            if node.is_restricted() {
                flags |= FLAG_RESTRICTED;
            }
            let mut suggestion: Option<&str> = node.suggester().map(|s| s.kind.id());
            let mut parser = None;
            if let NodeKind::Argument { ty, .. } = node.kind() {
                match registry.id(ty.parser_id()) {
                    Some(pid) => parser = Some((pid, ty)),
                    None => {
                        let fallback_id = registry.id(fallback.parser_id()).ok_or_else(|| {
                            SerializeError::MissingParser(fallback.parser_id().into())
                        })?;
                        unknown_parsers.insert(ty.parser_id().to_owned());
                        parser = Some((fallback_id, &fallback));
                        suggestion = Some(SuggestionKind::AskServer.id());
                    }
                }
                if suggestion.is_some() {
                    flags |= FLAG_SUGGESTIONS;
                }
            }
            body.push(flags);
            let children = &visible_children[&id];
            VarInt(children.len() as i32).encode(&mut body);
            for child in children {
                VarInt(index[child] as i32).encode(&mut body);
            }
            if let Some(r) = redirect {
                VarInt(r as i32).encode(&mut body);
            }
            if !matches!(node.kind(), NodeKind::Root) {
                node.name().encode(&mut body);
            }
            if let Some((pid, ty)) = parser {
                VarInt(pid).encode(&mut body);
                ty.write_properties(&mut body);
                if let Some(s) = suggestion {
                    s.encode(&mut body);
                }
            }
        }
        VarInt(0).encode(&mut body);
        Ok(SerializedCommands {
            body,
            node_count: order.len(),
            unknown_parsers,
        })
    }
}
