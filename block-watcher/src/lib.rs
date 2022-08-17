pub mod consumer;
pub mod producer;
mod statistics;

use core::{f64, time::Duration};

use anyhow::{anyhow, Context as _, Result as AnyResult};
use chrono::{TimeZone, Utc};
use futures::{FutureExt, StreamExt};
use statistics::{
    BLOCKS_ADDED, BLOCKS_RECEIVED, BLOCKS_REWOUND, BLOCK_HEADER_AGE, BLOCK_HEADER_LATENCY,
    BLOCK_TIME, CONNECTION_ATTEMPTS,
};
use thiserror::Error;
use tokio::{
    select, spawn,
    sync::broadcast::{channel, Receiver, Sender},
    time::{sleep, timeout},
};
use tracing::{debug, error, info};
use url::Url;
use web3::{
    api::{Eth, EthSubscribe, Namespace, SubscriptionStream},
    transports::WebSocket,
    types::{Block, BlockHeader, BlockId, BlockNumber, H256},
};

/// Max number of blocks in the event queue
const QUEUE_CAPACITY: usize = 20;

/// Time to wait on stream before trying to poll
const POLL_DELAY: Duration = Duration::from_secs(5);

/// Timeout on the [`Eth::block`] request
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum amount of success
const MAX_TRIES: usize = 10;

/// Time to wait between connection retries
const RETRY_DELAY: Duration = Duration::from_secs(1);

/// Maximum acceptable re-org size
const MAX_REORG: usize = 10;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reorgable<T> {
    Event(T),
    Reorg { block_height: u64 },
}

impl<T> From<T> for Reorgable<T> {
    fn from(event: T) -> Self {
        Self::Event(event)
    }
}

type Event = Reorgable<BlockHeader>;

#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names)]
enum Error {
    #[error("Web3 provider error")]
    Web3Error(#[from] web3::Error),
    #[error("Web3 provider timeout")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("Web3 provider error: stream closed")]
    EndOfStream,
    #[error("Web3 provider error: no last block")]
    NotFound,
    #[error("Invalid block received: number missing")]
    NumberMissing,
    #[error("Invalid block received: hash missing")]
    HashMissing,
    #[error("Re-org exceeded max re-org depth")]
    ReorgOverflow,
    #[error("Sanity check failed: parent hash mismatch")]
    InsaneParentHash,
    #[error("Sanity check failed: non-consecutive block numbers")]
    InsaneNumber,
}

/// Start blockwatcher task
pub fn start(url: Url) -> AnyResult<Receiver<Event>> {
    if !matches!(url.scheme(), "ws" | "wss") {
        return Err(anyhow!(
            "Unsupported ethereum transport {}. Use ws or wss.",
            url.scheme()
        ));
    }
    let (sender, receiver) = channel(QUEUE_CAPACITY);

    spawn(run(url, sender).map(|result| {
        if let Err(error) = result {
            error!(?error, "Error in task");
            std::process::abort();
        }
    }));

    Ok(receiver)
}

/// Run block watcher with retries
async fn run(url: Url, sender: Sender<Event>) -> AnyResult<()> {
    let mut last = None;
    let mut retries = 0;
    loop {
        let first = last.clone();
        let result = run_once(&url, &sender, &mut last).await;
        let error = match result {
            Ok(_) => return Ok(()),
            Err(e) => e,
        };
        error!(?error, "Block fetch connection failed");

        // Reset try counter if progress was made
        if last != first {
            retries = 0;
        }

        // Abort if maximum number of retries was exceeded
        if retries > MAX_TRIES {
            return Err(error).context("Maximum retries exceeded");
        }

        // Jitter delay.
        sleep(RETRY_DELAY).await;
        retries += 1;
    }
}

/// Handle a single connection lifecycle
async fn run_once(
    url: &Url,
    sender: &Sender<Event>,
    last: &mut Option<BlockHeader>,
) -> Result<(), Error> {
    // Connect to web3
    let (eth, mut sub) = connect(url).await?;

    // Fetch latest block if we don't have a last block
    if last.is_none() {
        let latest = fetch_header(&eth, BlockNumber::Latest).await?;
        // Send call returns error iif there are no receivers.
        // See <https://docs.rs/tokio/1.10.0/tokio/sync/broadcast/error/struct.SendError.html>
        let _result = sender.send(latest.clone().into());
        *last = Some(latest);
    }
    let last = last.as_mut().unwrap();

    // Fetch blocks
    fetch_loop(&eth, &mut sub, sender, last).await?;
    Ok(())
}

/// Create a new websocket connection
async fn connect(
    url: &Url,
) -> Result<(Eth<WebSocket>, SubscriptionStream<WebSocket, BlockHeader>), Error> {
    CONNECTION_ATTEMPTS.inc();
    let transport = WebSocket::new(url.as_str()).await?;
    let eth = Eth::new(transport.clone());
    let eth_subscribe = EthSubscribe::new(transport);
    let sub = eth_subscribe.subscribe_new_heads().await?;
    Ok((eth, sub))
}

/// Wait and poll for new blocks in a loop.
async fn fetch_loop(
    eth: &Eth<WebSocket>,
    sub: &mut SubscriptionStream<WebSocket, BlockHeader>,
    sender: &Sender<Event>,
    last: &mut BlockHeader,
) -> Result<(), Error> {
    loop {
        // Fetch next block and skip if not latest
        let block_timer = BLOCK_TIME.start_timer();
        let header = next_header(eth, sub).await?;
        let number = header.number.ok_or(Error::NumberMissing)?;
        if last.number.unwrap_or_default() >= number {
            debug!("Block is not on longest known chain, ignoring");
            continue;
        }
        drop(block_timer);

        // Log and measure block
        let hash = header.hash.ok_or(Error::HashMissing)?;
        #[allow(clippy::cast_possible_wrap)]
        let timestamp = Utc.timestamp(header.timestamp.as_u64() as i64, 0);
        let age = Utc::now() - timestamp;
        debug!(?number, ?hash, ?header, ?age, "Received header");
        BLOCK_HEADER_AGE.observe(age.to_std().unwrap_or_default().as_secs_f64());

        // Send block
        send_with_reorgs(eth, last, &header, sender).await?;
        *last = header;
    }
}

/// Send a new block on the channel including any reorg events
async fn send_with_reorgs(
    eth: &Eth<WebSocket>,
    last: &BlockHeader,
    latest: &BlockHeader,
    sender: &Sender<Event>,
) -> Result<(), Error> {
    let mut last = last.clone();
    let mut queue = vec![latest.clone()];
    let mut rewound = 0_usize;
    loop {
        if queue.len() > MAX_REORG {
            return Err(Error::ReorgOverflow);
        }
        let end = queue.last().unwrap();

        // Check if end of queue should connect to last
        if end.number.unwrap() == last.number.unwrap() + 1 {
            // Stop if the queue connects to `last`
            if end.parent_hash == last.hash.unwrap() {
                break;
            }

            // Rewind last to previous block (i.e. do a re-org)
            // TODO: Emit re-org event
            info!("Re-org detected, rewinding latest block");
            rewound += 1;
            last = fetch_header(eth, last.parent_hash).await?;
        }

        // Fetch previous
        let parent = fetch_header(eth, end.parent_hash).await?;
        queue.push(parent);
    }
    #[allow(clippy::cast_precision_loss)]
    BLOCKS_ADDED.observe(queue.len() as f64);
    BLOCKS_RECEIVED.inc_by(queue.len() as u64);
    if rewound > 0 {
        #[allow(clippy::cast_precision_loss)]
        BLOCKS_REWOUND.observe(rewound as f64);

        // Send re-org event
        let _result = sender.send(Reorgable::Reorg {
            block_height: last.number.unwrap().as_u64() + 1,
        });
    }

    // Send new headers to all receivers
    for header in queue.into_iter().rev() {
        // Sanity check
        if header.number.unwrap() != last.number.unwrap() + 1 {
            return Err(Error::InsaneNumber);
        }

        if header.parent_hash != last.hash.unwrap() {
            return Err(Error::InsaneParentHash);
        }
        last = header.clone();

        // Send call returns error iif there are no receivers.
        // See <https://docs.rs/tokio/1.10.0/tokio/sync/broadcast/error/struct.SendError.html>
        let _result = sender.send(header.into());
    }

    Ok(())
}

/// Try fetch the next header. If no new header is found in time, return the
/// last header.
async fn next_header(
    eth: &Eth<WebSocket>,
    sub: &mut SubscriptionStream<WebSocket, BlockHeader>,
) -> Result<BlockHeader, Error> {
    // Note that [`StreamExt::next`] is cancellation safe. We will not lose data
    // if we drop futures. See <https://docs.rs/tokio/1.10.0/tokio/macro.select.html#cancellation-safety>

    // Try waiting on the stream.
    select! {
        next = sub.next() => return Ok(next.ok_or(Error::EndOfStream)??),
        _ = sleep(POLL_DELAY) => {}
    }

    // Fetch and return the latest header instead. This also acts as a guarantee
    // that the websocket connection (which should be the same between [`eth`]
    // and [`sub`]) is still operational.
    // Note: The `fetch_header` call is not cancellable. When a block arrives
    // and the call gets dropped this will result in a harmless warning:
    // "web3::transports::ws: Sending a response to deallocated channel"
    select! {
        next = sub.next() => Ok(next.ok_or(Error::EndOfStream)??),
        last = fetch_header(eth, BlockNumber::Latest) => last
    }
}

async fn fetch_header<B: Into<BlockId> + Send>(
    eth: &Eth<WebSocket>,
    block_id: B,
) -> Result<BlockHeader, Error> {
    let _timer = BLOCK_HEADER_LATENCY.start_timer(); // Observe on drop
    let request = eth.block(block_id.into());
    let block = timeout(FETCH_TIMEOUT, request)
        .await??
        .ok_or(Error::NotFound)?;
    let header = block_to_header(block);
    let number = header.number.ok_or(Error::NumberMissing)?;
    let hash = header.hash.ok_or(Error::HashMissing)?;
    debug!(?number, ?hash, ?header, "Fetched header");
    Ok(header)
}

/// Convert a [`Block`] to [`BlockHeader`]
// Workaround, see <https://github.com/tomusdrw/rust-web3/issues/508>
fn block_to_header(block: Block<H256>) -> BlockHeader {
    BlockHeader {
        hash:              block.hash,
        parent_hash:       block.parent_hash,
        uncles_hash:       block.uncles_hash,
        author:            block.author,
        state_root:        block.state_root,
        transactions_root: block.transactions_root,
        receipts_root:     block.receipts_root,
        number:            block.number,
        gas_used:          block.gas_used,
        gas_limit:         block.gas_limit,
        base_fee_per_gas:  block.base_fee_per_gas,
        extra_data:        block.extra_data,
        logs_bloom:        block.logs_bloom.unwrap_or_default(),
        timestamp:         block.timestamp,
        difficulty:        block.difficulty,
        mix_hash:          block.mix_hash,
        nonce:             block.nonce,
    }
}
