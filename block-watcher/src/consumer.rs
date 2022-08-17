use anyhow::Result as AnyResult;
use futures::{Stream, StreamExt};
use types::{proto::BlockHeader as BlockHeaderProto, FromProto, Kafka, KafkaConsumer, Options};
use web3::types::BlockHeader;

pub struct Consumer(KafkaConsumer<BlockHeaderProto>);

impl Consumer {
    pub async fn new(input_topic: String, options: Options) -> AnyResult<Self> {
        let kafka = Kafka::new(options).await?;
        Ok(Self(kafka.new_consumer(&input_topic).await?))
    }

    pub fn stream(&self) -> impl Stream<Item = BlockHeader> + '_ {
        self.0.stream().map(|x| BlockHeader::from_proto(x.unwrap()))
    }
}
