use core::fmt::{Debug, Formatter, Result as FmtResult};
use std::{any::type_name, marker::PhantomData, sync::Arc};

use anyhow::{anyhow, Context as _, Error as AnyError, Result as AnyResult};
use futures::{stream::Stream, TryStreamExt};
use prost::Message;
use rdkafka::{
    consumer::{stream_consumer::StreamConsumer, Consumer},
    ClientConfig, Message as _,
};

use super::{storage::Storage, Kafka};
use crate::proto;

pub struct KafkaConsumer<T: Message + Default + Send + Sync> {
    client:   Kafka,
    consumer: Arc<StreamConsumer>,
    topic:    String,
    phantom:  PhantomData<T>,
}

impl<T: Message + Default + Send + Sync> Debug for KafkaConsumer<T> {
    fn fmt(&self, fmt: &mut Formatter) -> FmtResult {
        fmt.debug_tuple(type_name::<Self>())
            .field(&self.topic)
            .finish()
    }
}

impl<T: Message + Default + Send + Sync> Clone for KafkaConsumer<T> {
    /// Clone the consumer. Note that they share the underlying message stream,
    /// so messages will be read by at most one of the clones. To receive
    /// message everywhere, use [`Self::copy`].
    fn clone(&self) -> Self {
        self.share()
    }
}

impl<T: Message + Default + Send + Sync> KafkaConsumer<T> {
    pub fn new(client: &Kafka, topic: &str) -> AnyResult<Self> {
        let client = client.clone();
        let topic = topic.to_string();
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &client.options.kafka_brokers)
            .set("group.id", "Consumer")
            .create()
            .context("Error creating Kafka Consumer")?;
        consumer.subscribe(&[&topic])?;
        Ok(Self {
            client,
            consumer: Arc::new(consumer),
            topic,
            phantom: PhantomData,
        })
    }

    pub fn share(&self) -> Self {
        Self {
            client:   self.client.clone(),
            consumer: self.consumer.clone(),
            topic:    self.topic.clone(),
            phantom:  PhantomData,
        }
    }

    pub fn copy(&self) -> AnyResult<Self> {
        Self::new(&self.client, &self.topic)
    }

    pub fn stream(&self) -> impl Stream<Item = Result<T, AnyError>> + '_ {
        self.consumer.stream().err_into::<AnyError>().and_then({
            let topic = self.topic.clone();
            let storage = Arc::new(self.client.storage.clone());
            move |message| {
                let topic = topic.clone();
                let storage = storage.clone();
                async move {
                    let payload = message
                        .payload()
                        .ok_or_else(|| anyhow!("Kafka message missing payload"))?;
                    let message = Self::fetch(&topic, &storage, payload).await?;
                    Ok(message)
                }
            }
        })
    }

    pub async fn receive(&self) -> AnyResult<T> {
        let message = self.consumer.recv().await?;
        let payload = message
            .payload()
            .ok_or_else(|| anyhow!("Kafka message missing payload"))?;
        let message = Self::fetch(&self.topic, &self.client.storage, payload).await?;
        Ok(message)
    }

    async fn fetch(topic: &str, storage: &Storage, raw: &[u8]) -> AnyResult<T> {
        // Get the MaybeLarge message
        let maybe_large =
            proto::MaybeLarge::decode(raw).context("Error decoding MaybeLarge message")?;

        // Fetch the bytes for the embedded message (either directly or from storage)
        let bytes = match maybe_large.maybe_large {
            Some(proto::maybe_large::MaybeLarge::Embedded(bytes)) => bytes,
            Some(proto::maybe_large::MaybeLarge::Large(proto::Large { payload_path })) => {
                let topic_prefixed = format!("{}/{}", topic, &payload_path);
                storage.download(topic_prefixed).await?
            }
            None => {
                return Err(anyhow!("MaybeLarge message missing field maybe_large"));
            }
        };

        // Decode inner message
        let message = T::decode(bytes.as_slice())
            .with_context(|| format!("Error decoding {} message", type_name::<T>()))?;
        Ok(message)
    }
}
