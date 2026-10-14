use envconfig::Envconfig;
mod config;
use crate::config::Config;

// use rdkafka::client::ClientContext;
use rdkafka::config::{ClientConfig, RDKafkaLogLevel};
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::consumer::{CommitMode, Consumer};
use rdkafka::message::Message;

// A context can be used to change the behavior of producers and consumers by adding callbacks
// that will be executed by librdkafka.
// This particular context sets up custom callbacks to log rebalancing events.
// struct CustomContext;

// impl ClientContext for CustomContext {}

// impl ConsumerContext for CustomContext {
//     fn pre_rebalance(&self, _: &BaseConsumer<Self>, rebalance: &Rebalance) {
//         info!("Pre rebalance {:?}", rebalance);
//     }

//     fn post_rebalance(&self, _: &BaseConsumer<Self>, rebalance: &Rebalance) {
//         info!("Post rebalance {:?}", rebalance);
//     }

//     fn commit_callback(&self, result: KafkaResult<()>, _offsets: &TopicPartitionList) {
//         info!("Committing offsets: {:?}", result);
//     }
// }

// A type alias with your custom consumer can be created for convenience.
// type LoggingConsumer = StreamConsumer<CustomContext>;

async fn consume_and_print(config: &Config) {
    // let context = CustomContext;

    // let config = Config::init_from_env().expect("invalid configuration");
    let mut consumer_config = ClientConfig::new();     // ← this line is missing

    consumer_config
        .set("group.id", &config.group_id)
        .set("bootstrap.servers", &config.kafka_brokers)
        .set("enable.partition.eof", "false")
        .set("session.timeout.ms", "6000")
        .set("enable.auto.commit", "true")
        //.set("statistics.interval.ms", "30000")
        .set("auto.offset.reset", "earliest")
        .set_log_level(RDKafkaLogLevel::Debug);


    // TODO SHOULD NOT PANIC
    let consumer: StreamConsumer = consumer_config.create().expect("Error here");
    consumer
        .subscribe(&[config.kafka_events_topic.as_str()])
        .expect("Can't subscribe to specified topics");

    loop {
        match consumer.recv().await {
            // TODO(telemetry): tracing::warn!("Kafka error: {}", e)
            Err(_e) => todo!("log the client error once telemetry is wired"),
            Ok(m) => {
                let _payload = match m.payload_view::<str>() {
                    // ingest always produces a payload; a null value would come
                    // from something else writing to this topic.
                    None => unreachable!("murmur.events carries no tombstones"),
                    Some(Ok(s)) => s,
                    Some(Err(_e)) => {
                        // TODO(telemetry): tracing::warn! the bad payload and skip it
                        todo!("decide what a non-UTF-8 payload does")
                    }
                };

                // TODO(telemetry): tracing::info! key, payload, topic, partition,
                // offset and timestamp — see docs/tasks/medium/tracing-logging.md
                todo!("slice 5: print the message, then parse it into JetstreamMessage");

                #[allow(unreachable_code)]
                consumer.commit_message(&m, CommitMode::Async).unwrap();
            }
        };
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("ingestion-consumer: not implemented yet");
    println!("see docs/tasks/high/consumer-reads-topic.md");

    dotenvy::dotenv().ok();
    let config = Config::init_from_env().expect("invalid configuration");
    consume_and_print(&config).await;
    Ok(())
}
