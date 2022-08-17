mod consumer;
mod producer;
mod storage;

use std::time::Duration;

use anyhow::{Context, Result as AnyResult};
use prost::Message;
use rdkafka::{admin::AdminClient, metadata::MetadataTopic, ClientConfig};
use structopt::StructOpt;
use tokio::task::spawn_blocking;
use tracing::{debug, info};

use self::storage::Storage;
pub use self::{consumer::KafkaConsumer, producer::KafkaProducer};

const METADATA_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, StructOpt, PartialEq)]
pub struct Options {
    /// AWS S3 Storage options
    #[structopt(flatten)]
    storage: storage::Options,

    /// Comma separated bootstrap list of brokers
    #[structopt(long, env, default_value = "127.0.0.1:9092")]
    kafka_brokers: String,

    /// Threshold size in bytes where the Kafka message will be stored in AWS S3
    #[structopt(long, env, default_value = "500000")]
    kafka_large_message: usize,
}

#[derive(Clone)]
pub struct Kafka {
    options: Options,
    storage: Storage,
}

impl Kafka {
    pub async fn new(options: Options) -> AnyResult<Self> {
        // Create storage
        let storage = storage::Storage::new(options.storage.clone());

        // Test Kafka client config
        spawn_blocking({
            let brokers = options.kafka_brokers.clone();
            move || {
                info!("Connecting to Kafka at {}", &brokers);

                // See <https://docs.confluent.io/platform/current/clients/librdkafka/html/md_CONFIGURATION.html>
                let admin: AdminClient<_> = ClientConfig::new()
                    .set("bootstrap.servers", &brokers)
                    .create()
                    .with_context(|| format!("Error connecting to Kafka {}", &brokers))?;

                // Fetch metadata to test connection
                let meta = admin
                    .inner()
                    .fetch_metadata(None, METADATA_TIMEOUT)
                    .context("Error fetching metadata")?;

                debug!(
                    "Connected to broker {}: {}",
                    meta.orig_broker_id(),
                    meta.orig_broker_name()
                );
                debug!(
                    "Available brokers: {}",
                    meta.brokers()
                        .iter()
                        .map(|b| format!("{}: {}:{}", b.id(), b.host(), b.port()))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                debug!(
                    "Available topics: {:?}",
                    meta.topics()
                        .iter()
                        .map(MetadataTopic::name)
                        .collect::<Vec<_>>()
                );
                AnyResult::<(), anyhow::Error>::Ok(())
            }
        })
        .await??;

        Ok(Self { options, storage })
    }

    /// Create a new [`KafkaProducer`] for a given topic and type.
    pub async fn new_producer<T: Message + Default + Send + Sync>(
        &self,
        topic: &str,
    ) -> AnyResult<KafkaProducer<T>> {
        KafkaProducer::<T>::new(self, topic)
    }

    /// Create a new [`KafkaConsumer`] for a given topic and type.
    pub async fn new_consumer<T: Message + Default + Send + Sync>(
        &self,
        topic: &str,
    ) -> AnyResult<KafkaConsumer<T>> {
        KafkaConsumer::<T>::new(self, topic)
    }
}
