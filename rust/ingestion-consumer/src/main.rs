mod config;
use crate::config::Config;

use rdkafka::config::{ClientConfig, RDKafkaLogLevel};
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::consumer::{CommitMode, Consumer};
use rdkafka::message::Message;
use tracing::{debug, trace, warn};

async fn consume_and_print(config: &Config) {
    let mut consumer_config = ClientConfig::new();

    consumer_config
        .set("group.id", &config.group_id)
        .set("bootstrap.servers", &config.kafka_brokers)
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        .set("auto.offset.reset", "earliest")
        .set_log_level(RDKafkaLogLevel::Debug);

    let consumer: StreamConsumer = consumer_config
        .create()
        .expect("could not create the Kafka consumer");
    consumer
        .subscribe(&[config.kafka_events_topic.as_str()])
        .expect("Can't subscribe to specified topics");

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
                // TODO(exits): a failed commit kills a consumer that is working.
                consumer.commit_message(&m, CommitMode::Async).unwrap();
            }
        };
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config: Config = murmur_common::telemetry::bootstrap()?;

    consume_and_print(&config).await;
    Ok(())
}
