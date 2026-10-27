//! Ferrite server entry point.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    tracing::info!(
        "Ferrite for Minecraft {} (protocol {})",
        ferrite_protocol::GAME_VERSION,
        ferrite_protocol::PROTOCOL_VERSION
    );
    Ok(())
}
