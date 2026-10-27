use envconfig::Envconfig;
use tracing::{error, info, Level};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::EnvFilter;

/// A config that can report the level its subscriber should start at.
pub trait HasLogLevel {
    fn log_level(&self) -> Level;
}

/// Loads `.env`, builds the config from the environment, and starts the
/// subscriber.
pub fn bootstrap<C>() -> anyhow::Result<C>
where
    C: Envconfig + HasLogLevel,
{
    let dotenv = dotenvy::dotenv();
    let config = C::init_from_env()?;
    init(config.log_level());

    match dotenv {
        Ok(path) => info!("loaded environment from {}", path.display()),
        Err(e) => error!("could not load .env: {e}"),
    }

    Ok(config)
}

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
