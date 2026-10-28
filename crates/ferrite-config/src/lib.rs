//! Paper/Bukkit/vanilla-compatible server configuration for Ferrite.
//!
//! Ferrite aims to be a drop-in replacement for an existing Minecraft server directory, so
//! this crate models the files server owners already have:
//!
//! - `server.properties` (vanilla 26.3, all keys)
//! - `bukkit.yml`
//! - `spigot.yml`
//! - `config/paper-global.yml`
//! - `config/paper-world-defaults.yml`
//! - `<world>/paper-world.yml` (per-world overrides, falling back to the world defaults)
//!
//! Every file is loaded into a typed model with defaults, written back with comments, and
//! unknown keys are preserved on rewrite.
//!
//! ```no_run
//! use ferrite_config::{ConfigOverrides, FerriteConfig};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut config = FerriteConfig::load("server")?;
//! let overrides = ConfigOverrides::from_env()?;
//! config.apply_overrides(&overrides)?;
//! let nether = config.world_settings("world_nether");
//! println!("anti-xray: {}", nether.anticheat.anti_xray.enabled);
//! config.save()?;
//! # Ok(())
//! # }
//! ```

pub mod error;
pub mod migrate;
pub mod overrides;
pub mod properties;
pub mod server_properties;
pub mod yaml;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_norway::Value;

use crate::yaml::bukkit::BukkitConfig;
use crate::yaml::paper_global::PaperGlobal;
use crate::yaml::paper_world::WorldSettings;
use crate::yaml::spigot::SpigotConfig;

pub use crate::error::ConfigError;
pub use crate::overrides::ConfigOverrides;
pub use crate::properties::{PropertiesDocument, PropertyKey, PropertyValue};
pub use crate::server_properties::{
    Difficulty, Gamemode, RegionFileCompression, ServerProperties, ServerPropertiesDocument,
};
pub use crate::yaml::{
    ConfigFileKind, KeyComment, MaybeInt, YamlConfig, YamlDocument, emit_yaml_with_comments,
    load_raw, overlay,
};

/// All configuration files for one server directory.
#[derive(Debug, Clone)]
pub struct FerriteConfig {
    dir: PathBuf,
    server: ServerPropertiesDocument,
    bukkit: YamlDocument<BukkitConfig>,
    spigot: YamlDocument<SpigotConfig>,
    paper_global: YamlDocument<PaperGlobal>,
    paper_world_defaults: YamlDocument<WorldSettings>,
    overrides: ConfigOverrides,
}

impl FerriteConfig {
    /// Load every configuration file from `dir`, falling back to compiled-in defaults for
    /// files that do not exist. Unknown keys are retained for the next save.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let dir = dir.as_ref().to_path_buf();
        let server =
            ServerPropertiesDocument::load(dir.join("server.properties"))?.unwrap_or_default();
        Ok(Self {
            bukkit: load_or_default(&dir, ConfigFileKind::Bukkit)?,
            spigot: load_or_default(&dir, ConfigFileKind::Spigot)?,
            paper_global: load_or_default(&dir, ConfigFileKind::PaperGlobal)?,
            paper_world_defaults: load_or_default(&dir, ConfigFileKind::PaperWorldDefaults)?,
            server,
            dir,
            overrides: ConfigOverrides::new(),
        })
    }

    /// Load configuration and apply a set of overrides.
    pub fn load_with_overrides(
        dir: impl AsRef<Path>,
        overrides: &ConfigOverrides,
    ) -> Result<Self, ConfigError> {
        let mut config = Self::load(dir)?;
        config.apply_overrides(overrides)?;
        Ok(config)
    }

    /// Load configuration and apply overrides from the process environment.
    pub fn load_from_env(dir: impl AsRef<Path>) -> Result<Self, ConfigError> {
        Self::load_with_overrides(dir, &ConfigOverrides::from_env()?)
    }

    /// Build a configuration from compiled-in defaults, without touching the filesystem.
    pub fn defaults(dir: impl AsRef<Path>) -> Result<Self, ConfigError> {
        Ok(Self {
            dir: dir.as_ref().to_path_buf(),
            server: ServerPropertiesDocument::default(),
            bukkit: YamlDocument::defaults(ConfigFileKind::Bukkit)?,
            spigot: YamlDocument::defaults(ConfigFileKind::Spigot)?,
            paper_global: YamlDocument::defaults(ConfigFileKind::PaperGlobal)?,
            paper_world_defaults: YamlDocument::defaults(ConfigFileKind::PaperWorldDefaults)?,
            overrides: ConfigOverrides::new(),
        })
    }

    /// Create a defaults configuration and write every file into `dir`.
    pub fn save_defaults(dir: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let config = Self::defaults(dir.as_ref())?;
        config.save()?;
        Ok(config)
    }

    /// The server directory this configuration belongs to.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The typed `server.properties` model.
    pub fn server_properties(&self) -> &ServerProperties {
        self.server.typed()
    }

    /// The raw `server.properties` document, including unknown keys.
    pub fn server_properties_document(&self) -> &PropertiesDocument {
        self.server.document()
    }

    /// The typed `bukkit.yml` model.
    pub fn bukkit(&self) -> &BukkitConfig {
        self.bukkit.typed()
    }

    /// The typed `spigot.yml` model.
    pub fn spigot(&self) -> &SpigotConfig {
        self.spigot.typed()
    }

    /// The typed `config/paper-global.yml` model.
    pub fn paper_global(&self) -> &PaperGlobal {
        self.paper_global.typed()
    }

    /// The typed `config/paper-world-defaults.yml` model.
    pub fn paper_world_defaults(&self) -> &WorldSettings {
        self.paper_world_defaults.typed()
    }

    /// The overrides currently applied to this configuration.
    pub fn overrides(&self) -> &ConfigOverrides {
        &self.overrides
    }

    /// Resolve the settings for one world.
    ///
    /// Values in `<world>/paper-world.yml` override `config/paper-world-defaults.yml`;
    /// missing values fall back to the defaults. If the per-world file cannot be read or
    /// parsed, this returns the world defaults unchanged — use
    /// [`Self::try_world_settings`] when the error matters.
    pub fn world_settings(&self, world_name: &str) -> WorldSettings {
        self.try_world_settings(world_name)
            .unwrap_or_else(|_| self.paper_world_defaults.typed().clone())
    }

    /// Resolve the settings for one world, reporting parse errors.
    pub fn try_world_settings(&self, world_name: &str) -> Result<WorldSettings, ConfigError> {
        let mut value = self.paper_world_defaults.merged_value()?;
        let world_path = self.dir.join(world_name).join("paper-world.yml");

        if let Some(world_raw) = load_raw(&world_path, ConfigFileKind::PaperWorld)? {
            value = overlay(&value, &world_raw);
        }

        for (path, raw) in self.overrides.paper_world_defaults() {
            set_path(&mut value, path, raw)?;
        }
        if let Some(overrides) = self.overrides.worlds().get(world_name) {
            for (path, raw) in overrides {
                set_path(&mut value, path, raw)?;
            }
        }

        serde_norway::from_value(value).map_err(|error| ConfigError::yaml(&world_path, error))
    }

    /// Apply environment/CLI overrides on top of the loaded files.
    ///
    /// Overrides are stored so later calls to [`Self::world_settings`] see them too.
    pub fn apply_overrides(&mut self, overrides: &ConfigOverrides) -> Result<(), ConfigError> {
        for (key, value) in overrides.server() {
            self.server.set(key, value)?;
        }
        apply_yaml_overrides(&mut self.bukkit, overrides.bukkit())?;
        apply_yaml_overrides(&mut self.spigot, overrides.spigot())?;
        apply_yaml_overrides(&mut self.paper_global, overrides.paper_global())?;
        apply_yaml_overrides(
            &mut self.paper_world_defaults,
            overrides.paper_world_defaults(),
        )?;
        self.overrides.extend(overrides);
        Ok(())
    }

    /// Render one configuration file as Paper-style YAML with comments.
    ///
    /// Unknown keys loaded from disk are included. `PaperWorld` is rejected because a world
    /// name is required; use [`Self::world_to_yaml_with_comments`].
    pub fn to_yaml_with_comments(&self, file: ConfigFileKind) -> Result<String, ConfigError> {
        match file {
            ConfigFileKind::Bukkit => self.bukkit.to_yaml_with_comments(),
            ConfigFileKind::Spigot => self.spigot.to_yaml_with_comments(),
            ConfigFileKind::PaperGlobal => self.paper_global.to_yaml_with_comments(),
            ConfigFileKind::PaperWorldDefaults => self.paper_world_defaults.to_yaml_with_comments(),
            ConfigFileKind::PaperWorld => Err(ConfigError::Override {
                path: "paper-world".to_string(),
                message: "a world name is required; use world_to_yaml_with_comments".to_string(),
            }),
        }
    }

    /// Render the resolved settings for one world as a commented `paper-world.yml`.
    pub fn world_to_yaml_with_comments(&self, world_name: &str) -> Result<String, ConfigError> {
        let settings = self.try_world_settings(world_name)?;
        let value = serde_norway::to_value(&settings)
            .map_err(|error| ConfigError::serialize(error.to_string()))?;
        Ok(emit_yaml_with_comments(
            &value,
            ConfigFileKind::PaperWorld,
            WorldSettings::key_comments(),
        ))
    }

    /// Write every configuration file back to the directory it was loaded from.
    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_to(&self.dir)
    }

    /// Write every configuration file into `dir`.
    ///
    /// `server.properties` is written with one comment per key; YAML files are written in
    /// Paper style with comments above known keys and unknown keys preserved.
    pub fn save_to(&self, dir: impl AsRef<Path>) -> Result<(), ConfigError> {
        let dir = dir.as_ref();
        self.server.save(dir.join("server.properties"))?;
        self.bukkit
            .save(dir.join(ConfigFileKind::Bukkit.relative_path()))?;
        self.spigot
            .save(dir.join(ConfigFileKind::Spigot.relative_path()))?;
        self.paper_global
            .save(dir.join(ConfigFileKind::PaperGlobal.relative_path()))?;
        self.paper_world_defaults
            .save(dir.join(ConfigFileKind::PaperWorldDefaults.relative_path()))?;
        Ok(())
    }
}

fn load_or_default<T: YamlConfig>(
    dir: &Path,
    kind: ConfigFileKind,
) -> Result<YamlDocument<T>, ConfigError> {
    Ok(YamlDocument::load(dir.join(kind.relative_path()), kind)?
        .unwrap_or(YamlDocument::defaults(kind)?))
}

fn apply_yaml_overrides<T: YamlConfig>(
    document: &mut YamlDocument<T>,
    overrides: &BTreeMap<String, String>,
) -> Result<(), ConfigError> {
    for (path, raw) in overrides {
        let mut value = document.merged_value()?;
        set_path(&mut value, path, raw)?;
        let typed: T = serde_norway::from_value(value).map_err(|error| ConfigError::Override {
            path: path.clone(),
            message: error.to_string(),
        })?;
        document.set_typed(typed);
    }
    Ok(())
}

fn set_path(value: &mut Value, path: &str, raw: &str) -> Result<(), ConfigError> {
    if path.is_empty() {
        return Err(ConfigError::Override {
            path: path.to_string(),
            message: "empty override path".to_string(),
        });
    }

    let mut current = value;
    let segments: Vec<&str> = path.split('.').collect();
    for (index, segment) in segments.iter().enumerate() {
        let is_last = index + 1 == segments.len();
        let Some(mapping) = current.as_mapping_mut() else {
            return Err(ConfigError::Override {
                path: path.to_string(),
                message: format!("`{}` is not a mapping", segments[..index].join(".")),
            });
        };
        let Some(existing) = mapping.get_mut(*segment) else {
            return Err(ConfigError::Override {
                path: path.to_string(),
                message: format!("unknown key `{segment}`"),
            });
        };
        if is_last {
            *existing = coerce_override(existing, raw).ok_or_else(|| ConfigError::Override {
                path: path.to_string(),
                message: format!(
                    "cannot override `{segment}` with `{raw}` (expected the same scalar type as the file)"
                ),
            })?;
            return Ok(());
        }
        current = existing;
    }
    Ok(())
}

fn coerce_override(existing: &Value, raw: &str) -> Option<Value> {
    match existing {
        Value::Bool(_) => match raw.trim().to_ascii_lowercase().as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        Value::Number(number) => {
            if number.as_i64().is_some() {
                raw.trim().parse::<i64>().ok().map(Value::from)
            } else if number.as_u64().is_some() {
                raw.trim().parse::<u64>().ok().map(Value::from)
            } else {
                raw.trim().parse::<f64>().ok().map(Value::from)
            }
        }
        Value::String(_) => Some(Value::String(raw.to_string())),
        Value::Null => Some(Value::String(raw.to_string())),
        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_) => None,
    }
}
