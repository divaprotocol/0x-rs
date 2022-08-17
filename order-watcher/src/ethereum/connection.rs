//! Abstract the Web3 connection to transparently handle failing connections.

use std::sync::Arc;

use anyhow::{anyhow, Context as _, Result as AnyResult};
use futures::StreamExt;
use url::Url;
use web3::{
    api::SubscriptionStream,
    contract::Contract,
    transports::WebSocket,
    types::{Block, BlockHeader, BlockId, BlockNumber, H256, U256},
    Web3,
};
use crate::utils::AnyFlatten;

struct State {
    transport: WebSocket,
    exchange: Contract<WebSocket>,
    stream:   SubscriptionStream<WebSocket, BlockHeader>,
}

struct Connection {
    state: Arc<RwLock<State>>,
}

impl State {
    pub async fn connect(url: &Url) -> Self {
        let transport = match url.scheme() {
            "ws" | "wss" => {
                WebSocket::new(options.ethereum.as_str())
                    .await
                    .with_context(|| format!("Connecting to Ethereum at {:?}", options.ethereum))
            }
            other => {
                Err(anyhow!(
                    "Unsupported ethereum transport {}. Use ws or wss.",
                    other
                ))
            }
        }?;
    }
}

impl Connection {
    pub async fn connect(url: &Url) -> Self {
        todo!()
    }

    pub async fn chain_id(&self) -> AnyResult<U256> {
        // Eth is just a trivial wrapper around a Transport.
        Eth::new(self.state.

        Ok(self.state.web3.eth().chain_id().await?)
    }

    pub async fn wait_for_next_header(&self) -> AnyResult<BlockHeader> {
        // TODO: Requires mutability
        self.state
            .stream
            .next()
            .await
            .ok_or(anyhow!("Unexpected end of stream"))
            .any_flatten()
    }
}
