use anyhow::Error as AnyError;
use futures::TryStreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tracing::info;
use types::{proto::BlockHeader as BlockHeaderProto, IntoProto, Kafka, KafkaProducer, Options};
use url::Url;

use super::{start as start_watching, AnyResult, Reorgable};

// Maximum number of blocks to process concurrently
const MAX_CONCURRENT_BLOCKS: usize = 10;

pub async fn start(options: Options, url: Url, topic: String) -> AnyResult<()> {
    let block_watcher = Producer::new(options, topic).await?;
    block_watcher.start(url).await?;
    Ok(())
}

pub struct Producer(KafkaProducer<BlockHeaderProto>);

impl Producer {
    pub async fn new(options: Options, topic: String) -> AnyResult<Self> {
        let kafka = Kafka::new(options).await?;
        Ok(Self(kafka.new_producer(&topic).await?))
    }

    pub async fn start(&self, eth_url: Url) -> AnyResult<()> {
        let block_stream = BroadcastStream::new(start_watching(eth_url)?);
        block_stream
            .map_err(AnyError::from)
            .try_for_each_concurrent(Some(MAX_CONCURRENT_BLOCKS), move |event| {
                async move {
                    let header = match event {
                        Reorgable::Reorg { .. } => return Ok(()),
                        Reorgable::Event(header) => header,
                    };
                    info!(
                        "Sending block header with number = {:?} to Kafka",
                        header.number
                    );
                    self.0.send(&header.into_proto()).await?;
                    Ok(())
                }
            })
            .await?;
        Ok(())
    }
}
