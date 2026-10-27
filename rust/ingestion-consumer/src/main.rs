mod config;
use crate::config::Config;

use anyhow::Context;
use rdkafka::config::{ClientConfig, RDKafkaLogLevel};
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::consumer::{CommitMode, Consumer};
use rdkafka::message::Message;
use tracing::{debug, trace, warn};

async fn consume_and_print(config: &Config) -> anyhow::Result<()> {
    let mut consumer_config = ClientConfig::new();

    consumer_config
        .set("group.id", &config.group_id)
        .set("bootstrap.servers", &config.kafka_brokers)
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        .set("auto.offset.reset", "earliest")
        .set_log_level(RDKafkaLogLevel::Debug);

    let consumer: StreamConsumer = consumer_config.create().with_context(|| {
        format!(
            "could not create a Kafka consumer for {}",
            config.kafka_brokers
        )
    })?;
    consumer
        .subscribe(&[config.kafka_events_topic.as_str()])
        .with_context(|| format!("could not subscribe to {}", config.kafka_events_topic))?;

    loop {
        match consumer.recv().await {
            Err(e) => warn!("Kafka error: {}", e),
            Ok(m) => {
                let payload = match m.payload_view::<str>() {
                    None => "",
                    Some(Ok(s)) => s,
                    Some(Err(e)) => {
                        warn!("Error while deserializing message payload: {:?}", e);
                        ""
                    }
                };
                debug!(
                    "key: '{:?}', topic: {}, partition: {}, offset: {}, timestamp: {:?}",
                    m.key_view::<str>(),
                    m.topic(),
                    m.partition(),
                    m.offset(),
                    m.timestamp()
                );
                trace!("{payload}");
                if let Err(e) = consumer.commit_message(&m, CommitMode::Async) {
                    warn!("could not commit offset {}: {e}", m.offset());
                }
            }
        };
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config: Config = murmur_common::telemetry::bootstrap()?;

    consume_and_print(&config).await
}
