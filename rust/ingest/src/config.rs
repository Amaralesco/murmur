use envconfig::Envconfig;
use murmur_common::telemetry::HasLogLevel;
use std::num::NonZeroU64;
use std::path::PathBuf;
use std::str::FromStr;
use tracing::Level;

/// How often the archive starts a new file.
pub enum Rotation {
    Minutely,
    Hourly,
    Daily,
}

impl FromStr for Rotation {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "minutely" => Ok(Rotation::Minutely),
            "hourly" => Ok(Rotation::Hourly),
            "daily" => Ok(Rotation::Daily),
            other => Err(format!(
                "unknown ROTATION {other:?}: expected minutely, hourly or daily"
            )),
        }
    }
}

#[derive(Envconfig)]
pub struct Config {
    #[envconfig(default = "info")]
    pub log_level: Level,

    pub zone: String,

    pub jetstream_collections: String,

    pub kafka_brokers: String,

    #[envconfig(default = "1")]
    pub base_delay_secs: NonZeroU64,

    #[envconfig(default = "300")]
    pub max_delay_secs: NonZeroU64,

    #[envconfig(default = "2")]
    pub backoff_multiplier: NonZeroU64,

    #[envconfig(default = "5")]
    pub missed_pings_tolerance: NonZeroU64,
    // Todo: Make sure number of missed_pings_tolerance is at least 2
    #[envconfig(default = "data/readings")]
    pub readings_dir: PathBuf,

    #[envconfig(default = "hourly")]
    pub rotation: Rotation,

    // Avoid default so that Ingestion and Consumer share the same topic
    pub kafka_events_topic: String,
}

impl HasLogLevel for Config {
    fn log_level(&self) -> Level {
        self.log_level
    }
}

impl Config {
    /// `kinds=commit` is deliberately fixed: the `Commit` payload struct is only
    /// sound while that is the sole event kind requested.
    pub fn jetstream_url(&self) -> String {
        format!(
            "wss://jetstream.{}.bsky.network/xrpc/network.bsky.jetstream.subscribeEvents?kinds=commit&collections={}",
            self.zone, self.jetstream_collections
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn required_only() -> HashMap<String, String> {
        let mut vars = HashMap::new();
        vars.insert("ZONE".to_string(), "us-east".to_string());
        vars.insert(
            "JETSTREAM_COLLECTIONS".to_string(),
            "app.bsky.feed.post".to_string(),
        );
        vars.insert("KAFKA_BROKERS".to_string(), "localhost:9092".to_string());
        vars.insert(
            "KAFKA_EVENTS_TOPIC".to_string(),
            "murmur.events".to_string(),
        );
        vars
    }

    #[test]
    fn shipped_defaults_are_valid() {
        let result = Config::init_from_hashmap(&required_only());

        match result {
            Ok(_) => (),
            Err(e) => panic!("a default in Config does not parse: {e}"),
        }
    }
}
