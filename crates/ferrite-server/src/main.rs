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
    motd: String,
    max_players: u32,
    view_distance: u8,
    compression_threshold: usize,
    world: Option<std::path::PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:25565".into(),
            online_mode: true,
            motd: "A Ferrite server".into(),
            max_players: 20,
            view_distance: 10,
            compression_threshold: 256,
            world: None,
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
    let listener = tokio::net::TcpListener::bind(&config.bind_address)
        .await
        .context("binding server listener")?;
    tracing::info!(address = %listener.local_addr()?, online_mode = config.online_mode, view_distance = config.view_distance, "Ferrite listener started");

    let world = if let Some(path) = config.world {
        let spawn = game::load_spawn(&path).context("reading world spawn")?;
        let registry = ferrite_world::RuntimeRegistry::load("vanilla/26.3/generated")
            .context("loading vanilla registry")?;
        let source = game::new_anvil_source(path, registry);
        game::World::start_with(Some(source), spawn)
    } else {
        game::World::start()
    };
    let mut server = tokio::spawn(ferrite_net::serve(
        listener,
        ferrite_net::ServerConfig {
            motd: config.motd,
            max_players: config.max_players,
            online_mode: config.online_mode,
            encrypt_offline: false,
            compression_threshold: config.compression_threshold,
            view_distance: config.view_distance,
            game: Some(world.sender.clone()),
        },
    ));
    let result =
        tokio::select! { result=&mut server=>Some(result), _=tokio::signal::ctrl_c()=>None };
    world.shutdown().await?;
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
        assert_eq!(defaults.world, None);
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
