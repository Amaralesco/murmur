use envconfig::Envconfig;
use murmur_common::telemetry::HasLogLevel;
use tracing::Level;

#[derive(Envconfig)]
pub struct Config {
    #[envconfig(default = "info")]
    pub log_level: Level,

    pub kafka_brokers: String,

    pub kafka_events_topic: String,

    #[envconfig(default = "murmur-ingestion-consumer")]
    pub group_id: String,
}

impl HasLogLevel for Config {
    fn log_level(&self) -> Level {
        self.log_level
    }
}
