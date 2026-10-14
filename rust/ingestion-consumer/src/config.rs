use envconfig::Envconfig;

#[derive(Envconfig)]
pub struct Config {
    pub kafka_brokers: String,

    pub kafka_events_topic: String,

    #[envconfig(default = "murmur-ingestion-consumer")]
    pub group_id: String,
}