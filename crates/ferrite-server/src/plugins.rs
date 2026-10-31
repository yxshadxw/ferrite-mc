//! Loading plugins into the embedded JVM at server startup.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use ferrite_plugin::{PluginConfig, PluginHost, ServerHooks, ServerSnapshot, WorldSnapshot};

/// Everything the plugin bridge needs from the server's startup configuration.
pub struct Settings {
    pub directory: PathBuf,
    pub bridge_jar: PathBuf,
    pub motd: String,
    pub max_players: u16,
    pub port: u16,
    pub view_distance: u8,
    pub online_mode: bool,
    pub whitelist: bool,
    /// The main world, published before plugins can query the server.
    pub world: WorldSnapshot,
}

/// A running embedded JVM with the server's plugins enabled.
pub struct PluginBridge {
    host: Arc<PluginHost>,
}

impl PluginBridge {
    /// Start the JVM and enable every plugin jar in `settings.directory`.
    ///
    /// Returns `None` when the directory is absent or holds no jars, so a server
    /// without plugins never pays for JVM startup.
    pub fn start(settings: Settings) -> anyhow::Result<Option<Self>> {
        if !has_plugin_jars(&settings.directory) {
            return Ok(None);
        }
        let host = Arc::new(
            PluginHost::start(
                &settings.directory,
                PluginConfig {
                    bridge_jar: settings.bridge_jar.clone(),
                    server_snapshot: ServerSnapshot {
                        motd: settings.motd,
                        max_players: settings.max_players,
                        port: settings.port,
                        view_distance: settings.view_distance,
                        whitelist: settings.whitelist,
                        online_mode: settings.online_mode,
                        banned_ips: Vec::new(),
                    },
                    ..Default::default()
                },
            )
            .with_context(|| format!("loading plugins from {}", settings.directory.display()))?,
        );
        host.register_world(&settings.world)
            .context("publishing the main world to plugins")?;
        let bridge = Self { host };
        bridge.flush_logs();
        Ok(Some(bridge))
    }

    /// Server-side hooks that deliver lifecycle events to plugin listeners.
    pub fn hooks(&self) -> Arc<dyn ServerHooks> {
        let hooks: Arc<dyn ServerHooks> = self.host.clone();
        hooks
    }

    /// Disable every plugin, which runs their `onDisable` handlers.
    pub fn shutdown(self) -> anyhow::Result<()> {
        let result = self.host.shutdown();
        self.flush_logs();
        result.context("disabling plugins")
    }

    /// Forward plugin log messages to the server's tracing output.
    pub fn flush_logs(&self) {
        for line in self.host.drain_logs() {
            tracing::info!(target: "ferrite::plugin", "{line}");
        }
    }
}

fn has_plugin_jars(directory: &Path) -> bool {
    std::fs::read_dir(directory).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("jar"))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_jar_files_count_as_plugins() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_plugin_jars(dir.path()));
        std::fs::write(dir.path().join("readme.txt"), "not a plugin").unwrap();
        assert!(!has_plugin_jars(dir.path()));
        std::fs::write(dir.path().join("empty.jar"), "jar").unwrap();
        assert!(has_plugin_jars(dir.path()));
        assert!(!has_plugin_jars(&dir.path().join("missing")));
    }
}
