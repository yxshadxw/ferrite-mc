//! Ferrite server entry point.
use anyhow::Context;
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

    ferrite_net::serve(
        listener,
        ferrite_net::ServerConfig {
            motd: config.motd,
            max_players: config.max_players,
            online_mode: config.online_mode,
            encrypt_offline: false,
            compression_threshold: config.compression_threshold,
            view_distance: config.view_distance,
        },
    )
    .await
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
        let custom: Config =
            toml::from_str("bind-address = '127.0.0.1:0'\nmotd = 'Hello'\nmax-players = 42")
                .unwrap();
        assert_eq!(custom.motd, "Hello");
        assert_eq!(custom.max_players, 42);
    }
}
