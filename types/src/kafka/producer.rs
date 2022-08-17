use core::{
    fmt::{Debug, Formatter, Result as FmtResult},
    time::Duration,
};
use std::marker::PhantomData;

use anyhow::{Context as _, Result as AnyResult};
use chrono::{DateTime, SecondsFormat, Utc};
use prost::Message;
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use sha3::{Digest as _, Sha3_256};
use tracing::debug;

use super::Kafka;
use crate::proto;

const QUEUE_TIMEOUT: Duration = Duration::from_secs(5);

/// Kafka messages with the same key go to the same partition and are therefore
/// guaranteed to be delivered in order.
const PARTITION_KEY: &str = "order_watcher_events";

#[derive(Clone)]
pub struct KafkaProducer<T: Message + Default + Send + Sync> {
    client:   Kafka,
    producer: FutureProducer,
    topic:    String,
    phantom:  PhantomData<T>,
}

impl<T: Message + Default + Send + Sync> Debug for KafkaProducer<T> {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        fmt.debug_tuple("Kafka").field(&self.topic).finish()
    }
}

impl<T: Message + Default + Send + Sync> KafkaProducer<T> {
    pub fn new(client: &Kafka, topic: &str) -> AnyResult<Self> {
        let client = client.clone();
        let topic = topic.to_string();
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &client.options.kafka_brokers)
            .create()
            .context("Error creating Kafka Producer")?;
        Ok(Self {
            client,
            producer,
            topic,
            phantom: PhantomData,
        })
    }

    /// TODO: Reduce the allocations and copies / re-encodings of data.
    pub async fn send(&self, message: &T) -> AnyResult<()> {
        // Encode message
        let message = message.encode_to_vec();

        // If the message is to large, upload to object storage
        let message = if message.len() < self.client.options.kafka_large_message {
            let wrapped = proto::MaybeLarge {
                maybe_large: Some(proto::maybe_large::MaybeLarge::Embedded(message)),
            };
            wrapped.encode_to_vec()
        } else {
            self.upload_message(message).await?
        };

        let record = FutureRecord {
            topic:     &self.topic,
            partition: None,
            payload:   Some(&message),
            key:       Some(PARTITION_KEY),
            timestamp: Some(Utc::now().timestamp()),
            headers:   None,
        };
        let (partition, offset) = self
            .producer
            .send(record, QUEUE_TIMEOUT)
            .await
            .map_err(|(e, _)| e)
            .context("Error sending Kafka message")?;
        debug!(
            "Kafka message queued in partition {} offset {}",
            partition, offset
        );
        Ok(())
    }

    /// Upload encoded message and return encoded pointer message
    async fn upload_message(&self, message: Vec<u8>) -> AnyResult<Vec<u8>> {
        let name = object_name(Utc::now(), &message);

        // Upload with unique name
        let topic_prefixed = format!("{}/{}", self.topic, &name);
        self.client.storage.upload(topic_prefixed, message).await?;

        // Create a Large message variant
        let pointer = proto::MaybeLarge {
            maybe_large: Some(proto::maybe_large::MaybeLarge::Large(proto::Large {
                payload_path: name,
            })),
        };

        let message = pointer.encode_to_vec();
        Ok(message)
    }
}

/// Creates a unique name for the data.
///
/// The naming scheme is:
///
/// ```text
/// <year>/<iso date>/<iso datetime>-<sha3 content hash>
/// ```
fn object_name(when: DateTime<Utc>, data: &[u8]) -> String {
    // Compute blob hash
    let hash = {
        let mut hasher = Sha3_256::new();
        hasher.update(data);
        let hash = hasher.finalize();
        hex::encode(&hash)
    };

    format!(
        "{}/{}/{}-{}",
        when.format("%Y"),
        when.format("%F"),
        when.to_rfc3339_opts(SecondsFormat::Secs, true),
        hash
    )
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    #[test]
    fn test_blob_name() {
        let when = Utc.ymd(2021, 8, 31).and_hms(17, 41, 11);
        let blob = b"Hello, World!";
        assert_eq!(
            object_name(when, blob),
            "2021/2021-08-31/2021-08-31T17:41:\
             11Z-1af17a664e3fa8e419b8ba05c2a173169df76162a5a286e0c405b460d478f7ef"
        );
    }
}
