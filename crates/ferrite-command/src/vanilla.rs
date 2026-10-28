//! Builds a dispatcher from the data generator's `reports/commands.json`.
//!
//! The report gives node types, parsers + properties, `executable`, `redirect` paths and
//! `permissions` (`minecraft:require` + `minecraft:command_level`). It omits redirects to
//! the root (`execute run`, `return run`): a non-executable leaf without a redirect is a
//! dead end vanilla never registers, so the loader redirects such nodes to the root and
//! lists them in [`LoadReport::implicit_root_redirects`].

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::arguments::ArgumentType;
use crate::dispatcher::CommandDispatcher;
use crate::error::CommandError;
use crate::tree::{Command, Node, NodeId, NodeKind, Requirement};

/// Vanilla permission levels (`net.minecraft.server.permissions.PermissionLevel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PermissionLevel {
    All = 0,
    Moderators = 1,
    Gamemasters = 2,
    Admins = 3,
    Owners = 4,
}

impl PermissionLevel {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "all" => Self::All,
            "moderators" => Self::Moderators,
            "gamemasters" => Self::Gamemasters,
            "admins" => Self::Admins,
            "owners" => Self::Owners,
            _ => return None,
        })
    }

    pub fn from_level(level: u8) -> Option<Self> {
        Some(match level {
            0 => Self::All,
            1 => Self::Moderators,
            2 => Self::Gamemasters,
            3 => Self::Admins,
            4 => Self::Owners,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Moderators => "moderators",
            Self::Gamemasters => "gamemasters",
            Self::Admins => "admins",
            Self::Owners => "owners",
        }
    }
}

/// A command source with a vanilla permission level.
pub trait PermissionSource {
    fn has_permission(&self, level: PermissionLevel) -> bool;
}

/// What the loader did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// Nodes created (excluding the root).
    pub nodes: usize,
    /// Parser identifiers kept as [`ArgumentType::Opaque`].
    pub opaque_parsers: BTreeSet<String>,
    /// Leaf nodes redirected to the root (see module docs).
    pub implicit_root_redirects: Vec<Vec<String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("at /{path}: {message}")]
    Invalid { path: String, message: String },
}

fn invalid(path: &[String], message: impl Into<String>) -> LoadError {
    LoadError::Invalid {
        path: path.join("/"),
        message: message.into(),
    }
}

/// Maps a vanilla permission level to a node requirement.
pub type RequirementFactory<'a, S> = Box<dyn Fn(PermissionLevel) -> Requirement<S> + 'a>;
/// Creates the command for an executable node, given its path from the root.
pub type CommandFactory<'a, S> = Box<dyn FnMut(&[String]) -> Command<S> + 'a>;

/// Loader hooks: how to turn permissions into requirements and executable nodes into
/// commands.
pub struct LoadOptions<'a, S> {
    pub requirement: RequirementFactory<'a, S>,
    pub command: CommandFactory<'a, S>,
}

impl<'a, S: PermissionSource + 'static> Default for LoadOptions<'a, S> {
    /// Requirements check [`PermissionSource`]; commands fail with
    /// "Command not implemented: /path".
    fn default() -> Self {
        Self {
            requirement: Box::new(|level| Arc::new(move |s: &S| s.has_permission(level))),
            command: Box::new(stub_command),
        }
    }
}

/// A command that fails with `Command not implemented: /<path>`.
pub fn stub_command<S>(path: &[String]) -> Command<S> {
    let message = format!("Command not implemented: /{}", path.join(" "));
    Arc::new(move |_| Err(CommandError::failed(message.clone())))
}

struct PendingRedirect {
    node: NodeId,
    target: Vec<String>,
    path: Vec<String>,
}

impl<S> CommandDispatcher<S> {
    /// Adds the commands of a `commands.json` report under the root.
    pub fn load_vanilla_json(
        &mut self,
        json: &str,
        mut options: LoadOptions<'_, S>,
    ) -> Result<LoadReport, LoadError> {
        let root: Value = serde_json::from_str(json)?;
        if root.get("type").and_then(Value::as_str) != Some("root") {
            return Err(invalid(&[], "top-level node is not `root`"));
        }
        let mut report = LoadReport::default();
        let mut redirects = Vec::new();
        let mut path = Vec::new();
        if let Some(children) = root.get("children") {
            let children = children
                .as_object()
                .ok_or_else(|| invalid(&path, "`children` is not an object"))?;
            self.load_children(
                self.root(),
                children,
                &mut path,
                &mut options,
                &mut report,
                &mut redirects,
            )?;
        }
        for pending in redirects {
            let target = if pending.target.is_empty() {
                self.root()
            } else {
                self.find_node(&pending.target).ok_or_else(|| {
                    invalid(
                        &pending.path,
                        format!("redirect target /{} not found", pending.target.join("/")),
                    )
                })?
            };
            self.node_mut(pending.node).redirect = Some(target);
        }
        Ok(report)
    }

    /// Reads and loads `<reports_dir>/commands.json`.
    pub fn load_vanilla_dir(
        &mut self,
        reports_dir: impl AsRef<Path>,
        options: LoadOptions<'_, S>,
    ) -> Result<LoadReport, LoadError> {
        let json = std::fs::read_to_string(reports_dir.as_ref().join("commands.json"))?;
        self.load_vanilla_json(&json, options)
    }

    fn load_children(
        &mut self,
        parent: NodeId,
        children: &Map<String, Value>,
        path: &mut Vec<String>,
        options: &mut LoadOptions<'_, S>,
        report: &mut LoadReport,
        redirects: &mut Vec<PendingRedirect>,
    ) -> Result<(), LoadError> {
        for (name, value) in children {
            path.push(name.clone());
            self.load_node(parent, name, value, path, options, report, redirects)?;
            path.pop();
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn load_node(
        &mut self,
        parent: NodeId,
        name: &str,
        value: &Value,
        path: &mut Vec<String>,
        options: &mut LoadOptions<'_, S>,
        report: &mut LoadReport,
        redirects: &mut Vec<PendingRedirect>,
    ) -> Result<(), LoadError> {
        let kind = match value.get("type").and_then(Value::as_str) {
            Some("literal") => NodeKind::Literal(name.to_owned()),
            Some("argument") => {
                let parser = value
                    .get("parser")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid(path, "argument without `parser`"))?;
                let (ty, opaque) = ArgumentType::from_json(parser, value.get("properties"))
                    .map_err(|m| invalid(path, m))?;
                if opaque {
                    report.opaque_parsers.insert(parser.to_owned());
                }
                NodeKind::Argument {
                    name: name.to_owned(),
                    ty,
                }
            }
            other => return Err(invalid(path, format!("unexpected node type {other:?}"))),
        };
        if self.node(parent).child(name).is_some() {
            return Err(invalid(path, "duplicate child name"));
        }
        let mut node = Node::new(kind);
        if let Some(permissions) = value.get("permissions") {
            let level = parse_permission(permissions).map_err(|m| invalid(path, m))?;
            node.restricted = level > PermissionLevel::All;
            node.requirement = Some((options.requirement)(level));
        }
        if value.get("executable").and_then(Value::as_bool) == Some(true) {
            node.command = Some((options.command)(path));
        }
        let id = self.insert_node(parent, node);
        report.nodes += 1;

        let children = value.get("children").and_then(Value::as_object);
        match value.get("redirect") {
            Some(target) => {
                let target = target
                    .as_array()
                    .and_then(|a| {
                        a.iter()
                            .map(|v| v.as_str().map(str::to_owned))
                            .collect::<Option<Vec<_>>>()
                    })
                    .ok_or_else(|| invalid(path, "`redirect` is not a string array"))?;
                if children.is_some_and(|c| !c.is_empty()) {
                    return Err(invalid(path, "node has both children and a redirect"));
                }
                redirects.push(PendingRedirect {
                    node: id,
                    target,
                    path: path.clone(),
                });
            }
            None if children.is_none_or(Map::is_empty)
                && value.get("executable").and_then(Value::as_bool) != Some(true) =>
            {
                redirects.push(PendingRedirect {
                    node: id,
                    target: Vec::new(),
                    path: path.clone(),
                });
                report.implicit_root_redirects.push(path.clone());
            }
            None => {}
        }
        if let Some(children) = children {
            self.load_children(id, children, path, options, report, redirects)?;
        }
        Ok(())
    }
}

fn parse_permission(value: &Value) -> Result<PermissionLevel, String> {
    if value.get("type").and_then(Value::as_str) != Some("minecraft:require") {
        return Err(format!("unsupported permission check {value}"));
    }
    let permission = value
        .get("permission")
        .ok_or("`minecraft:require` without `permission`")?;
    if permission.get("type").and_then(Value::as_str) != Some("minecraft:command_level") {
        return Err(format!("unsupported permission {permission}"));
    }
    let level = permission
        .get("level")
        .and_then(Value::as_str)
        .ok_or("`command_level` without `level`")?;
    PermissionLevel::from_name(level).ok_or_else(|| format!("unknown permission level `{level}`"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Op(PermissionLevel);
    impl PermissionSource for Op {
        fn has_permission(&self, level: PermissionLevel) -> bool {
            self.0 >= level
        }
    }

    const SAMPLE: &str = r#"{
      "type": "root",
      "children": {
        "kill": {
          "type": "literal",
          "executable": true,
          "permissions": {"type": "minecraft:require", "permission": {"type": "minecraft:command_level", "level": "gamemasters"}},
          "children": {
            "targets": {"type": "argument", "parser": "minecraft:entity",
                        "properties": {"type": "entities", "amount": "multiple"}, "executable": true}
          }
        },
        "execute": {
          "type": "literal",
          "children": {
            "as": {"type": "literal", "children": {
              "targets": {"type": "argument", "parser": "minecraft:entity",
                          "properties": {"type": "entities", "amount": "multiple"}, "redirect": ["execute"]}}},
            "run": {"type": "literal"}
          }
        },
        "me": {"type": "literal", "children": {
          "action": {"type": "argument", "parser": "minecraft:message", "executable": true}}}
      }
    }"#;

    #[test]
    fn loads_sample() {
        let mut d = CommandDispatcher::<Op>::new();
        let report = d.load_vanilla_json(SAMPLE, LoadOptions::default()).unwrap();
        assert_eq!(report.nodes, 8);
        assert_eq!(report.implicit_root_redirects, vec![vec!["execute", "run"]]);
        let run = d.find_node(&["execute", "run"]).unwrap();
        assert_eq!(d.node(run).redirect(), Some(d.root()));
        let targets = d.find_node(&["execute", "as", "targets"]).unwrap();
        assert_eq!(d.node(targets).redirect(), d.find_node(&["execute"]));

        let kill = d.find_node(&["kill"]).unwrap();
        assert!(d.node(kill).is_restricted());
        assert!(!d.node(kill).can_use(&Op(PermissionLevel::All)));
        assert!(d.node(kill).can_use(&Op(PermissionLevel::Gamemasters)));

        let err = d.execute("kill", Op(PermissionLevel::Owners)).unwrap_err();
        assert_eq!(err.to_string(), "Command not implemented: /kill");
        let err = d
            .execute("execute as @a run me waves", Op(PermissionLevel::All))
            .unwrap_err();
        assert_eq!(err.to_string(), "Command not implemented: /me action");
    }

    #[test]
    fn rejects_bad_redirect() {
        let json = r#"{"type":"root","children":{"a":{"type":"literal","redirect":["nope"]}}}"#;
        let mut d = CommandDispatcher::<Op>::new();
        let err = d
            .load_vanilla_json(json, LoadOptions::default())
            .unwrap_err();
        assert!(err.to_string().contains("redirect target /nope not found"));
    }
}
