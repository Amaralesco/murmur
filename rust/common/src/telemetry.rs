use tracing::Level;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::EnvFilter;

/// Installs the process-wide tracing subscriber.
///
/// `default_level` is the baseline; `RUST_LOG`, when set, overrides it and can
/// target individual modules (`RUST_LOG=ingest=debug,tokio=warn`).
pub fn init(default_level: Level) {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::from_level(default_level).into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_target(true)
        .with_level(true)
        .with_env_filter(filter)
        .init();
}