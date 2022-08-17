//! Handle order state fetches in concurrent batches.

use std::{
    cmp::min,
    sync::{Arc, Mutex},
};

use once_cell::sync::Lazy;
use prometheus::{
    exponential_buckets, register_histogram, register_int_counter, register_int_counter_vec,
    Histogram, IntCounter, IntCounterVec,
};
use smallvec::{smallvec, SmallVec};
use thiserror::Error;
use tokio::{
    spawn,
    sync::{
        oneshot::{self, Sender},
        Notify, Semaphore,
    },
    time::{sleep, Duration},
};
use tracing::{info, trace};
use web3::{
    contract::{Contract, Options as Web3Options},
    transports::Http,
    types::{BlockId, BlockNumber},
};

use crate::{
    ethereum::{Input, Output},
    orders::{SignedOrder, SignedOrderState},
    require,
};

const QUEUE_CORK: Duration = Duration::from_millis(100);
const PRIORITY_CORK: Duration = Duration::from_millis(5);
const FUNC: &str = "batchGetLimitOrderRelevantStates";

static QUEUED: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "order_state_queued",
        "Count of orders queued for state fetching.",
        &["priority"]
    )
    .unwrap()
});
static MERGED: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "order_state_merged",
        "Count of order state requests deduplicated."
    )
    .unwrap()
});
static CALLED: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "order_state_called",
        "Count of order states requests called."
    )
    .unwrap()
});
static FETCHED: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!("order_state_fetched", "Count of order states fetched.").unwrap()
});
static CALLS: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "order_state_calls",
        "Count batchGetLimitOrderRelevantStates calls issued."
    )
    .unwrap()
});
static CALLS_COMPLETED: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "order_state_calls_completed",
        "Count batchGetLimitOrderRelevantStates calls completed."
    )
    .unwrap()
});
static BATCH_SIZE: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "order_state_batch_size",
        "The batchGetLimitOrderRelevantStates batch size.",
        exponential_buckets(1.0, 2.0, 10).unwrap()
    )
    .unwrap()
});
static LATENCY: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "order_state_latency_seconds",
        "The batchGetLimitOrderRelevantStates eth_call duration."
    )
    .unwrap()
});

type Job = (
    SignedOrder,
    SmallVec<[Sender<Result<SignedOrderState, Error>>; 1]>,
);

#[derive(Clone, Debug, Error)]
pub enum Error {
    #[error("Error in eth_call batchGetLimitOrderRelevantStates")]
    Web3Error(String),
    #[error("Invalid result from batchGetLimitOrderRelevantStates")]
    InvalidOutputLength,
}

#[derive(Debug, Default)]
struct State {
    priority: Vec<Job>,
    queue:    Vec<Job>,
}

#[derive(Debug)]
struct SyncState {
    state:      Mutex<State>,
    batch_size: usize,
    exchange:   Contract<Http>,
    notify:     Notify,
    semaphore:  Arc<Semaphore>, /* Even though SyncState is Arc, this is also Arc so that we can
                                 * use the acquire_owned method. */
}

#[derive(Clone, Debug)]
pub struct Batcher {
    sync: Arc<SyncState>,
}

impl State {
    #[allow(dead_code)]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn len(&self) -> usize {
        self.priority.len() + self.queue.len()
    }

    fn take_batch(&mut self, batch_size: usize) -> Vec<Job> {
        let mut result = Vec::with_capacity(batch_size);
        {
            let num = min(self.priority.len(), batch_size);
            result.extend(self.priority.drain(..num));
        }
        {
            let num = min(self.queue.len(), batch_size - result.len());
            result.extend(self.queue.drain(..num));
        }
        result
    }

    fn insert(&mut self, mut job: Job, priority: bool) {
        QUEUED
            .with_label_values(&[if priority { "true" } else { "false" }])
            .inc();
        if let Some(existing) = self.priority.iter_mut().find(|other| other.0 == job.0) {
            MERGED.inc();
            existing.1.append(&mut job.1);
        } else if let Some(existing) = self.queue.iter().position(|other| other.0 == job.0) {
            MERGED.inc();
            self.queue[existing].1.append(&mut job.1);
            if priority {
                self.priority.push(self.queue.remove(existing));
            }
        } else if priority {
            self.priority.push(job);
        } else {
            self.queue.push(job);
        }
    }
}

impl Batcher {
    pub fn new(exchange: Contract<Http>, batch_size: usize, concurrent: usize) -> Self {
        let batcher = Self {
            sync: Arc::new(SyncState {
                state: Mutex::default(),
                batch_size,
                exchange,
                notify: Notify::new(),
                semaphore: Arc::new(Semaphore::new(concurrent)),
            }),
        };
        // Spawn background task
        spawn({
            let batcher = batcher.clone();
            async move { batcher.run().await }
        });
        batcher
    }

    #[allow(clippy::large_types_passed_by_value)] // Takes ownership
    pub async fn fetch_state(
        &self,
        order: SignedOrder,
        priority: bool,
    ) -> Result<SignedOrderState, Error> {
        let (tx, rx) = oneshot::channel();
        let job = (order, smallvec![tx]);
        self.insert(job, priority);
        rx.await.unwrap()
    }

    fn insert(&self, job: Job, priority: bool) {
        let mut state = self.sync.state.lock().unwrap();
        let notify = if priority {
            state.priority.is_empty()
        } else {
            state.queue.is_empty()
        };
        state.insert(job, priority);
        let full = state.len() == self.sync.batch_size;
        if full {
            self.sync.notify.notify_one();
        } else if notify {
            let batcher = self.clone();
            spawn(async move {
                sleep(if priority { PRIORITY_CORK } else { QUEUE_CORK }).await;
                batcher.sync.notify.notify_one();
            });
        }
    }

    async fn run(&self) {
        info!("Batcher task starting");
        loop {
            // Wait for queue to have contents
            self.sync.notify.notified().await;
            loop {
                // Wait for connection to become available.
                // .unwrap() is safe because we never close the semaphore ourselves.
                let permit = self.sync.semaphore.clone().acquire_owned().await.unwrap();

                // Take next batch
                let batch = {
                    let mut state = self.sync.state.lock().unwrap();
                    state.take_batch(self.sync.batch_size)
                };
                // Note: If `self.sync.notify.notify_one()` is called here it will queue the
                // notice and `self.sync.notify.notified().await` will resolve immediately. So
                // there is no race condition.
                if batch.is_empty() {
                    break;
                }
                trace!("Processing batch size {}", batch.len());

                // Spawn processing
                let batcher = self.clone();
                spawn(async move {
                    let permit = permit;
                    // Batch process jobs
                    let input = batch.iter().map(|job| job.0).collect();
                    let result = batcher.fetch_batch_state(input).await;
                    drop(permit); // done with connection, add back permit

                    // Send results for all jobs in batch to all submitters
                    match result {
                        Ok(vec) => {
                            for (job, result) in batch.into_iter().zip(vec.into_iter()) {
                                for sender in job.1 {
                                    let _result = sender.send(Ok(result));
                                }
                            }
                        }
                        Err(err) => {
                            for job in batch {
                                for sender in job.1 {
                                    let _result = sender.send(Err(err.clone()));
                                }
                            }
                        }
                    }
                });
            }
        }
    }

    async fn fetch_batch_state(
        &self,
        orders: Vec<SignedOrder>,
    ) -> Result<Vec<SignedOrderState>, Error> {
        let _timer = LATENCY.start_timer();
        #[allow(clippy::cast_precision_loss)]
        BATCH_SIZE.observe(orders.len() as f64);
        CALLED.inc_by(orders.len() as u64);
        CALLS.inc();
        let len = orders.len();
        let from = None;
        let block_id = BlockId::from(BlockNumber::Latest);
        let options = Web3Options::default();
        let input = Input::from(orders);
        let output: Output = self
            .sync
            .exchange
            .query(FUNC, input, from, options, block_id)
            .await
            .map_err(|error| Error::Web3Error(error.to_string()))?;
        let output: Vec<SignedOrderState> = output.into();
        require!(output.len() == len, Error::InvalidOutputLength);
        FETCHED.inc_by(output.len() as u64);
        CALLS_COMPLETED.inc();
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::iter::repeat;

    use ethabi::Contract;
    use serde_json::{from_value, json};
    use web3::contract::tokens::Tokenize;

    use super::{super::EXCHANGE_ABI, *};

    fn abi() -> Contract {
        Contract::load(EXCHANGE_ABI).unwrap()
    }

    fn encode(orders: &[SignedOrder]) -> Vec<u8> {
        let abi = abi();
        let func_abi = abi.function(FUNC).unwrap();
        let input = Input::from(orders.to_vec());
        let tokens = input.into_tokens();
        let result = func_abi.encode_input(&tokens);
        result.unwrap()
    }

    fn example_order() -> SignedOrder {
        let json = json!({
            "makerToken": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
            "takerToken": "0xe41d2489571d322189246dafa5ebde1f4699f498",
            "makerAmount": "100000000000000",
            "takerAmount": "2000000000000000000000",
            "maker": "0x56EB0aD2dC746540Fab5C02478B31e2AA9DdC38C",
            "taker": "0x0000000000000000000000000000000000000000",
            "pool": "0x0000000000000000000000000000000000000000000000000000000000000000",
            "expiry": "1614956256",
            "salt": "2752094376750492926844965905320507011598275560670346196138937898764349624882",
            "chainId": 1,
            "verifyingContract": "0xdef1c0ded9bec7f1a1670819833240f027b25eff",
            "takerTokenFeeAmount": "0",
            "sender": "0x0000000000000000000000000000000000000000",
            "feeRecipient": "0x0000000000000000000000000000000000000000",
            "signature": {
                "v": 27,
                "r": "0x983a8a8dad663124a52609fe9aa82737f7f02d12ed951785f36b50906041794d",
                "s": "0x5f18ae837be4732bcb3dd019104cf775f92b8740b275be510462a7aa62cdf252",
                "signatureType": 3
            }
        });
        from_value::<SignedOrder>(json).unwrap()
    }

    #[test]
    fn test_abi_encoded_size() {
        let order = example_order();
        for num_orders in [0, 1, 2, 512] {
            let expected = 132 + num_orders * 512;
            let encoded = encode(&repeat(order).take(num_orders).collect::<Vec<_>>());
            dbg!(hex::encode(&encoded));
            assert_eq!(encoded.len(), expected);
        }
    }
}
