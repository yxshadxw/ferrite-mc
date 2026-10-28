//! Config version migration scaffolding.
//!
//! Paper stamps its YAML files with a config version (`_version_`, or `config-version` in
//! older files). Ferrite records a small schema version of its own and migrates older files
//! forward one step at a time. Files stamped with a newer version (for example a Paper
//! release Ferrite predates) are left untouched rather than rewritten with guesses.
//!
//! To add a migration, append a [`MigrationStep`] to the appropriate table with the next
//! `from_version`/`to_version` pair and bump [`ConfigFileKind::supported_version`].

use serde_norway::{Mapping, Value};

use crate::yaml::ConfigFileKind;

/// One forward migration from `from_version` to `to_version`.
#[derive(Debug, Clone, Copy)]
pub struct MigrationStep {
    /// Version this step applies to.
    pub from_version: u32,
    /// Version produced by this step.
    pub to_version: u32,
    /// Human-readable description, reported by [`migrate`].
    pub description: &'static str,
    /// The transformation.
    pub apply: fn(&mut Value),
}

/// What [`migrate`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// The file is already at the supported version (or has no config version).
    UpToDate,
    /// The file was migrated forward.
    Migrated {
        /// Version found in the file.
        from: u32,
        /// Version written back.
        to: u32,
        /// Descriptions of the steps that ran.
        applied: Vec<&'static str>,
    },
    /// The file is newer than this build; it was not modified.
    NewerThanSupported {
        /// Version found in the file.
        found: u32,
        /// Newest version this build knows.
        supported: u32,
    },
}

/// Migrations for `config/paper-global.yml`.
pub const PAPER_GLOBAL_MIGRATIONS: &[MigrationStep] = &[MigrationStep {
    from_version: 0,
    to_version: 1,
    description: "move commands.time-command-affects-all-worlds to time.affects-all-worlds",
    apply: move_time_command_setting,
}];

/// Migrations for `paper-world-defaults.yml` / `paper-world.yml`.
pub const PAPER_WORLD_MIGRATIONS: &[MigrationStep] = &[];

/// Migrate a raw YAML document for `kind` in place.
pub fn migrate(kind: ConfigFileKind, value: &mut Value) -> MigrationOutcome {
    let supported = kind.supported_version();
    let steps = match kind {
        ConfigFileKind::PaperGlobal => PAPER_GLOBAL_MIGRATIONS,
        ConfigFileKind::PaperWorldDefaults | ConfigFileKind::PaperWorld => PAPER_WORLD_MIGRATIONS,
        ConfigFileKind::Bukkit | ConfigFileKind::Spigot => return MigrationOutcome::UpToDate,
    };

    let found = read_version(value);
    if found > supported {
        return MigrationOutcome::NewerThanSupported { found, supported };
    }
    if found == supported {
        return MigrationOutcome::UpToDate;
    }

    let mut version = found;
    let mut applied = Vec::new();
    while version < supported {
        let Some(step) = steps.iter().find(|step| step.from_version == version) else {
            break;
        };
        (step.apply)(value);
        applied.push(step.description);
        version = step.to_version;
    }
    write_version(value, supported);

    MigrationOutcome::Migrated {
        from: found,
        to: supported,
        applied,
    }
}

/// Read the config version from a YAML mapping.
pub fn read_version(value: &Value) -> u32 {
    let Some(mapping) = value.as_mapping() else {
        return 0;
    };
    for key in ["_version_", "config-version"] {
        if let Some(version) = mapping.get(key).and_then(Value::as_u64) {
            return u32::try_from(version).unwrap_or(u32::MAX);
        }
    }
    0
}

fn write_version(value: &mut Value, version: u32) {
    let Some(mapping) = value.as_mapping_mut() else {
        return;
    };
    // Only one version key may remain: serde would see the alias twice and reject the file.
    mapping.remove("config-version");
    let key = Value::String("_version_".to_string());
    match mapping.get_mut(&key) {
        Some(existing) => *existing = Value::from(version),
        None => {
            mapping.insert(key, Value::from(version));
        }
    }
}

fn move_time_command_setting(value: &mut Value) {
    let Some(mapping) = value.as_mapping_mut() else {
        return;
    };
    let legacy = mapping
        .get_mut("commands")
        .and_then(Value::as_mapping_mut)
        .and_then(|commands| commands.remove("time-command-affects-all-worlds"));
    let Some(legacy) = legacy else {
        return;
    };

    let time = mapping
        .entry(Value::String("time".to_string()))
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if let Some(time) = time.as_mapping_mut() {
        time.entry(Value::String("affects-all-worlds".to_string()))
            .or_insert(legacy);
    }
}
