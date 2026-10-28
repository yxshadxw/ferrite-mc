//! Environment and command-line overrides.
//!
//! Overrides are collected as dotted paths and applied after files are loaded, so a
//! container or launcher can tweak a value without rewriting the owner's files:
//!
//! ```text
//! --set server.motd=Welcome
//! --set paper-global.proxies.velocity.enabled=true
//! --set paper-world-defaults.anticheat.anti-xray.enabled=true
//! --set world.world_nether.hopper.disable-move-event=false
//! ```
//!
//! Environment equivalents:
//!
//! - `FERRITE_SERVER_<KEY>` for `server.properties`, with `-` and `.` written as `_`
//!   (for example `FERRITE_SERVER_MAX_PLAYERS=50`).
//! - `FERRITE_CONFIG_OVERRIDES` for everything else: semicolon-separated assignments in the
//!   same `section.path=value` form as `--set`.

use std::collections::BTreeMap;

use crate::{ConfigError, ServerProperties};

/// A set of overrides to apply after configuration files are loaded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigOverrides {
    server: BTreeMap<String, String>,
    bukkit: BTreeMap<String, String>,
    spigot: BTreeMap<String, String>,
    paper_global: BTreeMap<String, String>,
    paper_world_defaults: BTreeMap<String, String>,
    worlds: BTreeMap<String, BTreeMap<String, String>>,
}

impl ConfigOverrides {
    /// No overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether no overrides were collected.
    pub fn is_empty(&self) -> bool {
        self.server.is_empty()
            && self.bukkit.is_empty()
            && self.spigot.is_empty()
            && self.paper_global.is_empty()
            && self.paper_world_defaults.is_empty()
            && self.worlds.is_empty()
    }

    /// Override one `server.properties` key.
    pub fn set_server(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.server.insert(key.into(), value.into());
    }

    /// Override one key in `bukkit.yml`.
    pub fn set_bukkit(&mut self, path: impl Into<String>, value: impl Into<String>) {
        self.bukkit.insert(path.into(), value.into());
    }

    /// Override one key in `spigot.yml`.
    pub fn set_spigot(&mut self, path: impl Into<String>, value: impl Into<String>) {
        self.spigot.insert(path.into(), value.into());
    }

    /// Override one key in `config/paper-global.yml`.
    pub fn set_paper_global(&mut self, path: impl Into<String>, value: impl Into<String>) {
        self.paper_global.insert(path.into(), value.into());
    }

    /// Override one key in `config/paper-world-defaults.yml`.
    pub fn set_paper_world_defaults(&mut self, path: impl Into<String>, value: impl Into<String>) {
        self.paper_world_defaults.insert(path.into(), value.into());
    }

    /// Override one key in a world's `paper-world.yml`.
    pub fn set_world(
        &mut self,
        world: impl Into<String>,
        path: impl Into<String>,
        value: impl Into<String>,
    ) {
        self.worlds
            .entry(world.into())
            .or_default()
            .insert(path.into(), value.into());
    }

    /// Merge another override set into this one. Values from `other` win.
    pub fn extend(&mut self, other: &Self) {
        self.server.extend(other.server.clone());
        self.bukkit.extend(other.bukkit.clone());
        self.spigot.extend(other.spigot.clone());
        self.paper_global.extend(other.paper_global.clone());
        self.paper_world_defaults
            .extend(other.paper_world_defaults.clone());
        for (world, overrides) in &other.worlds {
            self.worlds
                .entry(world.clone())
                .or_default()
                .extend(overrides.clone());
        }
    }

    /// `server.properties` overrides.
    pub fn server(&self) -> &BTreeMap<String, String> {
        &self.server
    }

    /// `bukkit.yml` overrides, keyed by dotted path.
    pub fn bukkit(&self) -> &BTreeMap<String, String> {
        &self.bukkit
    }

    /// `spigot.yml` overrides, keyed by dotted path.
    pub fn spigot(&self) -> &BTreeMap<String, String> {
        &self.spigot
    }

    /// `config/paper-global.yml` overrides, keyed by dotted path.
    pub fn paper_global(&self) -> &BTreeMap<String, String> {
        &self.paper_global
    }

    /// `config/paper-world-defaults.yml` overrides, keyed by dotted path.
    pub fn paper_world_defaults(&self) -> &BTreeMap<String, String> {
        &self.paper_world_defaults
    }

    /// Per-world overrides, keyed by world name and then dotted path.
    pub fn worlds(&self) -> &BTreeMap<String, BTreeMap<String, String>> {
        &self.worlds
    }

    /// Parse one `section.path=value` assignment.
    pub fn apply_assignment(&mut self, assignment: &str) -> Result<(), ConfigError> {
        let Some((path, value)) = assignment.split_once('=') else {
            return Err(ConfigError::Override {
                path: assignment.to_string(),
                message: "expected `section.path=value`".to_string(),
            });
        };
        let path = path.trim();
        if path.is_empty() {
            return Err(ConfigError::Override {
                path: assignment.to_string(),
                message: "missing section/path".to_string(),
            });
        }
        let value = value.to_string();

        if path == "server" {
            return Err(ConfigError::Override {
                path: path.to_string(),
                message: "missing key after `server.`".to_string(),
            });
        }
        if let Some(key) = path.strip_prefix("server.") {
            self.set_server(key, value);
            return Ok(());
        }
        for (prefix, target) in [
            ("bukkit.", &mut self.bukkit),
            ("spigot.", &mut self.spigot),
            ("paper-global.", &mut self.paper_global),
            ("paper-world-defaults.", &mut self.paper_world_defaults),
        ] {
            if let Some(key) = path.strip_prefix(prefix) {
                target.insert(key.to_string(), value);
                return Ok(());
            }
        }
        if let Some(rest) = path.strip_prefix("world.") {
            let Some((world, key)) = rest.split_once('.') else {
                return Err(ConfigError::Override {
                    path: path.to_string(),
                    message: "expected `world.<name>.<path>`".to_string(),
                });
            };
            self.set_world(world, key, value);
            return Ok(());
        }

        Err(ConfigError::Override {
            path: path.to_string(),
            message: "unknown section; use server, bukkit, spigot, paper-global, \
                      paper-world-defaults, or world.<name>"
                .to_string(),
        })
    }

    /// Collect overrides from command-line arguments.
    ///
    /// Recognizes `--set section.path=value` and `--set=section.path=value`; all other
    /// arguments are ignored.
    pub fn from_cli_args<I, S>(args: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut overrides = Self::new();
        let mut args = args.into_iter().peekable();
        while let Some(arg) = args.next() {
            let arg = arg.as_ref();
            if let Some(assignment) = arg.strip_prefix("--set=") {
                overrides.apply_assignment(assignment)?;
            } else if arg == "--set" {
                let Some(assignment) = args.next() else {
                    return Err(ConfigError::Override {
                        path: "--set".to_string(),
                        message: "missing `section.path=value` argument".to_string(),
                    });
                };
                overrides.apply_assignment(assignment.as_ref())?;
            }
        }
        Ok(overrides)
    }

    /// Collect overrides from the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_env_vars(std::env::vars())
    }

    /// Collect overrides from an arbitrary environment-like iterator.
    ///
    /// Split out from [`Self::from_env`] so tests do not need to mutate the process
    /// environment.
    pub fn from_env_vars<I, K, V>(vars: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut overrides = Self::new();
        let server_keys: BTreeMap<String, &'static str> = ServerProperties::KEY_TABLE
            .iter()
            .map(|entry| (server_env_name(entry.key), entry.key))
            .collect();

        for (name, value) in vars {
            let name = name.as_ref();
            let value = value.as_ref();
            if name == "FERRITE_CONFIG_OVERRIDES" {
                for assignment in value.split(';') {
                    let assignment = assignment.trim();
                    if !assignment.is_empty() {
                        overrides.apply_assignment(assignment)?;
                    }
                }
                continue;
            }
            if name.starts_with("FERRITE_SERVER_") {
                match server_keys.get(name) {
                    Some(key) => overrides.set_server(*key, value),
                    None => {
                        return Err(ConfigError::Override {
                            path: name.to_string(),
                            message: "unknown server.properties key".to_string(),
                        });
                    }
                }
            }
        }
        Ok(overrides)
    }
}

/// The environment variable name for a `server.properties` key.
pub fn server_env_name(key: &str) -> String {
    let suffix = key.replace(['-', '.'], "_").to_ascii_uppercase();
    format!("FERRITE_SERVER_{suffix}")
}
