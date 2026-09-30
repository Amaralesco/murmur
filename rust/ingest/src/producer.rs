// Kafka producer for `murmur.events`.

use anyhow::Context;
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;

/// Fixed while there is one topic. It moves into `Config` when the overflow and
/// historical topics in `architecture.md` arrive, because the choice between
/// them is then a routing decision rather than a constant.
const TOPIC: &str = "murmur.events";

/// How long `send` waits for room in the producer's *internal queue* before
/// giving up. Not the delivery deadline — that is `message.timeout.ms` below.
const ENQUEUE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct EventProducer {
    producer: FutureProducer,
}

impl EventProducer {
    /// `brokers` is a comma-separated list of `host:port` used only to
    /// bootstrap. The broker answers with the addresses to use from then on,
    /// which is why the value differs by where the client runs:
    /// `localhost:9092` from the host, `kafka:29092` from inside the Compose
    /// network.
    pub fn new(brokers: &str) -> anyhow::Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            // librdkafka retries internally. This is the total budget before a
            // message is declared undeliverable, not a per-attempt timeout.
            .set("message.timeout.ms", "5000")
            .create()
            .with_context(|| format!("could not create a Kafka producer for {brokers}"))?;

        Ok(Self { producer })
    }

    /// Sends one message and waits for the broker to acknowledge it, returning
    /// the partition and offset it was written to.
    ///
    /// Awaiting is the whole point. `send` only *queues* the message in a
    /// background buffer; a process that returns from here without awaiting,
    /// and then exits, delivers nothing while reporting success.
    pub async fn send(&self, key: &str, payload: &str) -> anyhow::Result<(i32, i64)> {
        let record = FutureRecord::to(TOPIC).key(key).payload(payload);

        match self.producer.send(record, ENQUEUE_TIMEOUT).await {
            Ok(delivery) => Ok((delivery.partition, delivery.offset)),
            Err((error, _returned_message)) => Err(anyhow::Error::new(error)
                .context(format!("Kafka delivery failed for key {key} on {TOPIC}"))),
        }
    }
}
