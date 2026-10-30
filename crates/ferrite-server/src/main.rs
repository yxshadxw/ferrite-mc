//! Ferrite server entry point.
use anyhow::Context;
use ferrite_server::game;
use serde::Deserialize;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Config {
    bind_address: String,
    online_mode: bool,
    enforce_secure_profile: bool,
    motd: String,
    max_players: u32,
    view_distance: u8,
    compression_threshold: usize,
    forwarding: ForwardingMode,
    forwarding_secret: String,
    world: Option<std::path::PathBuf>,
    gamemode: GameMode,
    ops: Vec<OpConfig>,
    plugins_dir: Option<std::path::PathBuf>,
    plugin_bridge: Option<std::path::PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpConfig {
    Uuid(String),
    Detailed {
        uuid: String,
        #[serde(default = "default_op_level")]
        level: u8,
    },
}
fn default_op_level() -> u8 {
    4
}
fn build_forwarding(
    mode: ForwardingMode,
    secret: &str,
) -> anyhow::Result<ferrite_net::forwarding::Forwarding> {
    Ok(match mode {
        ForwardingMode::None => ferrite_net::forwarding::Forwarding::None,
        ForwardingMode::Bungee => ferrite_net::forwarding::Forwarding::Bungee,
        ForwardingMode::Velocity => {
            anyhow::ensure!(
                !secret.is_empty(),
                "forwarding-secret must be set when forwarding = 'velocity'"
            );
            ferrite_net::forwarding::Forwarding::Velocity {
                secret: secret.to_owned(),
            }
        }
    })
}
fn parse_ops(ops: Vec<OpConfig>) -> anyhow::Result<Vec<game::Operator>> {
    ops.into_iter()
        .map(|op| {
            let (id, level) = match op {
                OpConfig::Uuid(id) => (id, 4),
                OpConfig::Detailed { uuid, level } => (uuid, level),
            };
            anyhow::ensure!(
                level <= 4,
                "operator permission level must be between 0 and 4"
            );
            let uuid = uuid::Uuid::parse_str(&id)
                .with_context(|| format!("invalid operator UUID: {id}"))?;
            Ok(game::Operator { uuid, level })
        })
        .collect()
}
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum GameMode {
    Survival,
    #[default]
    Creative,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ForwardingMode {
    #[default]
    None,
    Velocity,
    Bungee,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:25565".into(),
            online_mode: true,
            enforce_secure_profile: true,
            motd: "A Ferrite server".into(),
            max_players: 20,
            view_distance: 10,
            compression_threshold: 256,
            forwarding: ForwardingMode::None,
            forwarding_secret: String::new(),
            world: None,
            gamemode: GameMode::Creative,
            ops: Vec::new(),
            plugins_dir: Some("plugins".into()),
            plugin_bridge: None,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let config: Config = match std::fs::read_to_string("ferrite.toml") {
        Ok(content) => toml::from_str(&content).context("invalid ferrite.toml")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Config::default(),
        Err(error) => return Err(error).context("reading ferrite.toml"),
    };
    let ops = parse_ops(config.ops)?;
    let forwarding = build_forwarding(config.forwarding, &config.forwarding_secret)?;
    if forwarding.is_proxy() && config.online_mode {
        tracing::warn!(
            "online-mode is ignored when player forwarding is enabled; authenticate players on the proxy"
        );
    }
    if !config.online_mode && !ops.is_empty() {
        tracing::warn!(
            "operator UUIDs in offline mode must be offline-mode UUIDs; player names are not authenticated"
        );
    }
    let listener = tokio::net::TcpListener::bind(&config.bind_address)
        .await
        .context("binding server listener")?;
    tracing::info!(address = %listener.local_addr()?, online_mode = config.online_mode, view_distance = config.view_distance, "Ferrite listener started");

    let plugin_bridge =
        ferrite_server::plugins::PluginBridge::start(ferrite_server::plugins::Settings {
            directory: config.plugins_dir.unwrap_or_else(|| "plugins".into()),
            bridge_jar: config.plugin_bridge.unwrap_or_else(|| {
                "java/ferrite-api-impl/build/libs/ferrite-api-impl-all.jar".into()
            }),
            motd: config.motd.clone(),
            max_players: u16::try_from(config.max_players).unwrap_or(u16::MAX),
            port: listener.local_addr()?.port(),
            view_distance: config.view_distance,
            online_mode: config.online_mode,
            whitelist: false,
        })
        .context("starting plugins")?;
    let settings = game::WorldSettings {
        gamemode: match config.gamemode {
            GameMode::Survival => 0,
            GameMode::Creative => 1,
        },
        ops,
        plugins: plugin_bridge
            .as_ref()
            .map(ferrite_server::plugins::PluginBridge::hooks),
    };
    game::playerdata::init(
        config
            .world
            .as_ref()
            .map_or_else(|| "playerdata".into(), |w| w.join("playerdata")),
    );
    let world = if let Some(path) = config.world {
        let spawn = game::load_spawn(&path).context("reading world spawn")?;
        let registry = ferrite_world::RuntimeRegistry::load("vanilla/26.3/generated")
            .context("loading vanilla registry")?;
        let source = game::new_anvil_source(path, registry);
        game::World::start_configured(Some(source), spawn, settings)
    } else {
        game::World::start_configured(None, [0, 65, 0], settings)
    };
    let mut server = tokio::spawn(ferrite_net::serve(
        listener,
        ferrite_net::ServerConfig {
            motd: config.motd,
            max_players: config.max_players,
            online_mode: config.online_mode,
            encrypt_offline: false,
            enforce_secure_profile: config.enforce_secure_profile,
            compression_threshold: config.compression_threshold,
            view_distance: config.view_distance,
            forwarding,
            game: Some(world.sender.clone()),
        },
    ));
    let (console_tx, mut console_rx) = tokio::sync::mpsc::unbounded_channel();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::stdin().lock().lines() {
            match line {
                Ok(line) => {
                    if console_tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let result = loop {
        tokio::select! {
            result = &mut server => break Some(result),
            _ = tokio::signal::ctrl_c() => break None,
            Some(command) = console_rx.recv() => match world.console_command(command).await {
                Ok(true) => break None,
                Ok(false) => {},
                Err(error) => tracing::warn!(%error, "console command failed"),
            },
        }
    };
    let world_result = world.shutdown().await;
    if let Some(bridge) = plugin_bridge
        && let Err(error) = bridge.shutdown()
    {
        tracing::error!(%error, "plugin shutdown failed");
    }
    world_result?;
    if let Some(result) = result {
        result??;
    } else {
        server.abort();
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_defaults_and_overrides() {
        let defaults: Config = toml::from_str("").unwrap();
        assert_eq!(defaults.bind_address, "0.0.0.0:25565");
        assert_eq!(defaults.max_players, 20);
        assert_eq!(defaults.compression_threshold, 256);
        assert_eq!(defaults.forwarding, ForwardingMode::None);
        assert!(defaults.forwarding_secret.is_empty());
        assert_eq!(defaults.world, None);
        assert!(matches!(
            build_forwarding(ForwardingMode::None, "").unwrap(),
            ferrite_net::forwarding::Forwarding::None
        ));
        assert!(build_forwarding(ForwardingMode::Velocity, "").is_err());
        assert_eq!(
            build_forwarding(ForwardingMode::Velocity, "hunter2").unwrap(),
            ferrite_net::forwarding::Forwarding::Velocity {
                secret: "hunter2".into()
            }
        );
        assert_eq!(
            defaults.plugins_dir.as_deref(),
            Some(std::path::Path::new("plugins"))
        );
        assert_eq!(defaults.plugin_bridge, None);
        assert!(parse_ops(vec![OpConfig::Uuid("Alice".into())]).is_err());
        let parsed = parse_ops(vec![OpConfig::Detailed {
            uuid: "00000000-0000-0000-0000-00000000000a".into(),
            level: 3,
        }])
        .unwrap();
        assert_eq!(parsed[0].level, 3);
        assert_eq!(parsed[0].uuid, uuid::Uuid::from_u128(10));
        let custom: Config =
            toml::from_str("bind-address = '127.0.0.1:0'\nmotd = 'Hello'\nmax-players = 42")
                .unwrap();
        assert_eq!(custom.motd, "Hello");
        assert_eq!(custom.max_players, 42);
        let configured: Config = toml::from_str("world = 'fixture-world'").unwrap();
        assert_eq!(
            configured.world.as_deref(),
            Some(std::path::Path::new("fixture-world"))
        );
    }
}
