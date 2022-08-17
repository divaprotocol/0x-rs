#![doc = include_str!("../Readme.md")]
#![warn(clippy::all, clippy::pedantic, clippy::cargo, clippy::nursery)]

#[macro_use]
extern crate diesel;

mod api;
mod database;
mod ethereum;
mod logging;
mod orders;
mod utils;

use std::net::SocketAddr;

use anyhow::{Context as _, Result as AnyResult};
use api::Error as ApiError;
use block_watcher::{self, consumer::Consumer as BlockConsumer};
use chrono::offset::Utc;
use ethabi::Address;
use futures::stream::{self, StreamExt as _, TryStreamExt as _};
use once_cell::sync::Lazy;
use prometheus::{
    exponential_buckets, register_histogram, register_histogram_vec, register_int_counter,
    register_int_counter_vec, Histogram, HistogramVec, IntCounter, IntCounterVec,
};
use structopt::StructOpt;
use tokio::{sync::oneshot, try_join};
use tracing::{error, info, trace, warn};
use types::{proto::zeroex::OrderEvent, IntoProto, KafkaProducer};
use web3::types::U64;

use crate::{
    database::Database,
    ethereum::Ethereum,
    orders::{Metadata, OrderStatus, SignedOrder, SignedOrderWithMetadata},
    utils::spawn_or_abort,
};

// Maximum number of blocks to process concurrently
const MAX_CONCURRENT_BLOCKS: usize = 10;

static REVALIDATION_LATENCY: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "core_revalidation_latency",
        "Time it takes to revalidate orders.",
        exponential_buckets(0.1, 2.0, 12).unwrap()
    )
    .unwrap()
});
static INVALIDATION_REASON: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "core_invalidation_reason",
        "Count of invalidated orders by reason.",
        &["reason"]
    )
    .unwrap()
});
static UNINVALIDATED: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "core_uninvalidated",
        "Count of invalid orders becoming valid again."
    )
    .unwrap()
});
static REVALIDATION_STEP_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "core_revalidation_step_duration",
        "Time it takes to revalidate orders.",
        &["step"],
        exponential_buckets(0.1, 2.0, 12).unwrap()
    )
    .unwrap()
});

#[derive(Debug, PartialEq, StructOpt)]
pub struct Options {
    #[structopt(flatten)]
    database: database::Options,

    #[structopt(flatten)]
    ethereum: ethereum::Options,

    #[structopt(flatten)]
    kafka: types::Options,

    #[structopt(long, env = "ORDER_EVENT_TOPIC", default_value = "order_events")]
    order_event_topic: String,

    #[structopt(
        long,
        env = "BLOCK_WATCHER_TOPIC",
        default_value = "block_watcher_events"
    )]
    block_watcher_topic: String,

    /// DevUtils contract address.
    #[structopt(
        long,
        env = "DEV_UTILS",
        default_value = "0xDef1C0ded9bec7F1a1670819833240f027b25EfF"
    )]
    dev_utils: Address,

    /// Order submission server socket address
    #[structopt(long, env = "SUBMIT_SERVER", default_value = "127.0.0.1:8080")]
    submit_server: SocketAddr,
}

#[derive(Clone, Debug)]
struct App {
    database: Database,
    ethereum: Ethereum,
    kafka:    types::KafkaProducer<OrderEvent>,
}

impl App {
    async fn connect(options: Options) -> AnyResult<Self> {
        let (ethereum, kafka) = try_join!(
            Ethereum::connect(options.ethereum),
            new_producer(options.kafka, options.order_event_topic),
        )?;
        let database = Database::connect(options.database, ethereum.chain.chain_id).await?;
        Ok(Self {
            database,
            ethereum,
            kafka,
        })
    }

    #[allow(clippy::large_types_passed_by_value)]
    async fn order(&self, order: SignedOrder) -> Result<(), ApiError> {
        let received = Utc::now();

        // Validate order and fetch state
        order
            .order
            .validate(&self.ethereum.chain)
            .map_err(|e| ApiError::OrderInvalid(vec![e.into()]))?;
        let state = self
            .ethereum
            .batcher
            .fetch_state(order, true)
            .await
            .map_err(|error| {
                error!(?error, "Error fetching order state");
                ApiError::InternalError
            })?;
        state
            .validate()
            .map_err(|e| ApiError::OrderInvalid(vec![e.into()]))?;

        // Add metadata
        let signed_order_with_metadata = SignedOrderWithMetadata {
            signed_order: order,
            metadata:     Metadata {
                hash:       state.hash,
                remaining:  state.taker_asset_fillable_amount,
                status:     OrderStatus::Added,
                created_at: received,
            },
        };

        // Insert into database
        self.database
            .insert_order(signed_order_with_metadata)
            .await
            .map_err(|error| {
                error!(?error, "Error inserting order");
                ApiError::InternalError
            })?;

        // Emit event
        self.kafka
            .send(&signed_order_with_metadata.into_proto())
            .await
            .map_err(|error| {
                error!(?error, "Error emitting order event");
                ApiError::InternalError
            })?;

        Ok(())
    }

    async fn orders(&self, orders: Vec<SignedOrder>) -> Result<(), ApiError> {
        // Process many orders concurrently
        const CONCURRENT: usize = 32;
        let results = stream::iter(orders.into_iter())
            .map(|order| self.order(order))
            .buffered(CONCURRENT)
            .collect::<Vec<_>>()
            .await;
        let mut errors = vec![];
        let mut failed = false;
        for result in results {
            match result {
                Err(ApiError::OrderInvalid(errs)) => {
                    failed = true;
                    errors.extend(errs);
                }
                Err(e) => {
                    return Err(e);
                }
                Ok(_) => {}
            }
        }
        if failed {
            Err(ApiError::OrderInvalid(errors))
        } else {
            Ok(())
        }
    }

    #[allow(clippy::large_types_passed_by_value)] // Takes ownership
    async fn revalidate(&self, order: SignedOrderWithMetadata, block_number: U64) -> AnyResult<()> {
        let _timer = REVALIDATION_STEP_DURATION // Observes on drop
            .with_label_values(&["revalidate_one"])
            .start_timer();

        // TODO: Don't revalidate if a job is already pending.
        let was_invalid = order.metadata.status != OrderStatus::Fillable;

        // Fetch new state
        let step_timer = REVALIDATION_STEP_DURATION // Observes on drop
            .with_label_values(&["fetch_state"])
            .start_timer();
        let new_state = self
            .ethereum
            .batcher
            .fetch_state(order.signed_order, false)
            .await?;
        let mut new_order = order;
        new_order.metadata.remaining = new_state.taker_asset_fillable_amount;
        new_order.metadata.status = new_state.status;
        drop(step_timer);

        // Emit Kafka event if status changed (but not if it changed from one
        // unfillable state to another)
        if new_order != order && (!was_invalid || new_state.status == OrderStatus::Fillable) {
            let _step_timer = REVALIDATION_STEP_DURATION // Observes on drop
                .with_label_values(&["kafka_event"])
                .start_timer();
            self.kafka.send(&new_order.into_proto()).await?;
        }

        // Update database
        let hash = order.metadata.hash;
        let step_timer = REVALIDATION_STEP_DURATION // Observes on drop
            .with_label_values(&["validate"])
            .start_timer();
        let validity = new_state.validate();
        drop(step_timer);
        match validity {
            Ok(()) => {
                if was_invalid || order.metadata.remaining != new_order.metadata.remaining {
                    UNINVALIDATED.inc();
                    let _step_timer = REVALIDATION_STEP_DURATION // Observes on drop
                        .with_label_values(&["update_order"])
                        .start_timer();
                    self.database
                        .update_order(hash, new_order.metadata.remaining)
                        .await?;
                }
            }
            Err(reason) => {
                if !was_invalid {
                    INVALIDATION_REASON
                        .with_label_values(&[reason.into()])
                        .inc();
                    let _step_timer =
                        REVALIDATION_STEP_DURATION // Observes on drop
                            .with_label_values(&["invalidate_order"])
                            .start_timer();
                    self.database.invalidate_order(hash, block_number).await?;
                }
            }
        }
        Ok(())
    }
}

#[allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]
pub async fn main(options: Options, shutdown: oneshot::Receiver<()>) -> AnyResult<()> {
    let serve_url = options.submit_server;
    let max_reorg = options.ethereum.max_reorg;
    let block_watcher_kafka = options.kafka.clone();
    let block_watcher_topic = options.block_watcher_topic.clone();

    let app = App::connect(options).await?;

    // Green thread to re-validate orders on new blocks
    spawn_or_abort({
        let app = app.clone();
        async move {
            let app = app.clone();
            let block_consumer =
                BlockConsumer::new(block_watcher_topic, block_watcher_kafka).await?;
            let block_stream = block_consumer.stream();
            block_stream
                .map(Ok)
                .try_for_each_concurrent(Some(MAX_CONCURRENT_BLOCKS), move |header| {
                    let app = app.clone();
                    async move {
                        info!(
                            number = ?header.number.unwrap_or_default(),
                            hash = ?header.hash.unwrap_or_default(),
                            "Received block header",
                        );
                        let _timer = REVALIDATION_LATENCY.start_timer(); // Observes on drop
                        trace!("Revalidating all orders");

                        // Delete invalid orders that are older than the maximum re-org depth.
                        let step_timer = REVALIDATION_STEP_DURATION
                            .with_label_values(&["delete"])
                            .start_timer();
                        let block_number = header.number.unwrap();
                        app.database.delete_orders(block_number - max_reorg).await?;
                        drop(step_timer);

                        // Fetch all orders
                        let step_timer = REVALIDATION_STEP_DURATION
                            .with_label_values(&["get_orders"])
                            .start_timer();
                        let signed_order_with_metadatas =
                            app.database.get_orders(&app.ethereum.chain).await?;
                        drop(step_timer);

                        // Handle concurrently
                        let step_timer = REVALIDATION_STEP_DURATION
                            .with_label_values(&["revalidate_all"])
                            .start_timer();
                        stream::iter(signed_order_with_metadatas.into_iter())
                            .map(Ok)
                            .try_for_each_concurrent(None, |order| {
                                let step_timer = REVALIDATION_STEP_DURATION
                                    .with_label_values(&["clone"])
                                    .start_timer();
                                let app = app.clone(); // TODO: Perf?
                                drop(step_timer);
                                async move { app.revalidate(order, block_number).await }
                            })
                            .await
                            .context("Error revalidating orders")?;
                        drop(step_timer);
                        trace!("Revalidation done.");
                        Ok(())
                    }
                })
                .await
        }
    });

    // Start submit server
    spawn_or_abort(async move {
        api::serve(app, &serve_url).await?;
        AnyResult::Ok(())
    });

    // Wait for shutdown
    info!("Order watcher started, waiting for shutdown signal");
    shutdown.await?;
    // TODO: Graceful shutdown

    Ok(())
}

async fn new_producer(
    options: types::Options,
    topic: String,
) -> AnyResult<KafkaProducer<OrderEvent>> {
    let kafka = types::Kafka::new(options).await?;
    Ok(kafka.new_producer(&topic).await?)
}

#[cfg(test)]
pub mod test {
    use pretty_assertions::assert_eq;
    use proptest::proptest;
    use tracing::{error, warn};
    use tracing_test::traced_test;

    use super::*;

    #[test]
    #[allow(clippy::eq_op)]
    fn test_with_proptest() {
        proptest!(|(a in 0..5, b in 0..5)| {
            assert_eq!(a + b, b + a);
        });
    }

    #[test]
    #[traced_test]
    fn test_with_log_output() {
        error!("logged on the error level");
        assert!(logs_contain("logged on the error level"));
    }

    #[tokio::test]
    #[traced_test]
    #[allow(clippy::semicolon_if_nothing_returned)] // False positive
    async fn async_test_with_log() {
        // Local log
        info!("This is being logged on the info level");

        // Log from a spawned task (which runs in a separate thread)
        tokio::spawn(async {
            warn!("This is being logged on the warn level from a spawned task");
        })
        .await
        .unwrap();

        // Ensure that `logs_contain` works as intended
        assert!(logs_contain("logged on the info level"));
        assert!(logs_contain("logged on the warn level"));
        assert!(!logs_contain("logged on the error level"));
    }
}

#[cfg(feature = "bench")]
pub mod bench {
    use std::time::Duration;

    use criterion::{black_box, BatchSize, Criterion};
    use proptest::{
        strategy::{Strategy, ValueTree},
        test_runner::TestRunner,
    };
    use tokio::runtime;

    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub fn main(criterion: &mut Criterion) {
        orders::bench::group(criterion);
        utils::bench::group(criterion);
        bench_example_proptest(criterion);
        bench_example_async(criterion);
    }

    /// Constructs an executor for async tests
    pub(crate) fn runtime() -> runtime::Runtime {
        runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    /// Example proptest benchmark
    /// Uses proptest to randomize the benchmark input
    fn bench_example_proptest(criterion: &mut Criterion) {
        let input = (0..5, 0..5);
        let mut runner = TestRunner::deterministic();
        // Note: benchmarks need to have proper identifiers as names for
        // the CI to pick them up correctly.
        criterion.bench_function("example_proptest", move |bencher| {
            bencher.iter_batched(
                || input.new_tree(&mut runner).unwrap().current(),
                |(a, b)| {
                    // Benchmark number addition
                    black_box(a + b)
                },
                BatchSize::LargeInput,
            );
        });
    }

    /// Example async benchmark
    /// See <https://bheisler.github.io/criterion.rs/book/user_guide/benchmarking_async.html>
    fn bench_example_async(criterion: &mut Criterion) {
        let duration = Duration::from_micros(1);
        criterion.bench_function("example_async", move |bencher| {
            bencher.to_async(runtime()).iter(|| {
                async {
                    tokio::time::sleep(duration).await;
                }
            });
        });
    }
}
