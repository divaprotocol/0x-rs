mod queryable;
mod schema;

use core::fmt::Debug;
use std::{
    convert::TryFrom,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context as _, Result as AnyResult};
use diesel::{
    debug_query, delete, insert_into,
    pg::{Pg, PgConnection},
    prelude::*,
    update,
};
use once_cell::sync::Lazy;
use prometheus::{
    exponential_buckets, register_histogram, register_histogram_vec, register_int_counter_vec,
    register_int_gauge, Histogram, HistogramVec, IntCounterVec, IntGauge,
};
use structopt::StructOpt;
use tokio::task::spawn_blocking;
use tracing::{info, trace};
use url::Url;
use web3::types::{H256, U128, U256, U64};

pub use self::schema::signed_orders_v4;
use crate::{
    ethereum::ChainInfo,
    orders::Signature,
    utils::{Any as _, AnyFlatten as _},
    SignedOrderWithMetadata,
};

static OPS_COUNTER: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!("db_operations", "Database operations by kind.", &["kind"]).unwrap()
});
static LATENCY: Lazy<Histogram> =
    Lazy::new(|| register_histogram!("db_latency_seconds", "The DB latency in seconds.").unwrap());
static ORDERS: Lazy<IntGauge> =
    Lazy::new(|| register_int_gauge!("db_orders", "Number of orders in the database.").unwrap());
static STEP_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "db_get_orders_step_duration",
        "Time it takes to get all orders.",
        &["step"],
        exponential_buckets(0.1, 2.0, 12).unwrap()
    )
    .unwrap()
});

#[derive(Clone, PartialEq, Debug, StructOpt)]
pub struct Options {
    /// Database connection string.
    // Default from docker_compose.yaml in 0x-api
    // See <https://github.com/0xProject/0x-api/blob/2c329591/docker-compose.yml#L11>
    #[structopt(
        short,
        long,
        env = "DATABASE",
        default_value = "postgres://postgres:postgres@localhost/diva-api"
    )]
    pub database: Url,
}

#[derive(Clone)]
pub struct Database {
    url:        Url,
    connection: Arc<Mutex<PgConnection>>,
    chain_id:   U256,
}

impl Debug for Database {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::fmt::Result {
        fmt.debug_tuple("Database").field(&self.url).finish()
    }
}

impl Database {
    pub async fn connect(options: Options, chain_id: U256) -> AnyResult<Self> {
        info!("Connecting to PostgreSQL at {}", &options.database);
        let connection = spawn_blocking({
            let url = options.database.clone();
            move || PgConnection::establish(url.as_str())
        })
        .await
        .any_flatten()
        .with_context(|| format!("Error connecting to database {}", options.database))?;
        Ok(Self {
            url: options.database,
            connection: Arc::new(Mutex::new(connection)),
            chain_id,
        })
    }

    pub async fn get_orders(&self, chain: &ChainInfo) -> AnyResult<Vec<SignedOrderWithMetadata>> {
        OPS_COUNTER.with_label_values(&["get_orders"]).inc();
        let _timer = STEP_DURATION // Observes on drop
            .with_label_values(&["total"])
            .start_timer();

        trace!("Fetching orders from database");
        let step_timer = STEP_DURATION // Observes on drop
            .with_label_values(&["postgres"])
            .start_timer();
        let mut signed_orders_with_metadatas = self
            .with_connection(move |connection| {
                signed_orders_v4::table
                    .load::<SignedOrderWithMetadata>(connection)
                    .any()
            })
            .await
            .context("error in get_order_and_metadatas query")?;
        drop(step_timer);
        ORDERS.set(signed_orders_with_metadatas.len() as i64);
        trace!(
            "Received {} orders from database",
            signed_orders_with_metadatas.len()
        );

        let step_timer = STEP_DURATION // Observes on drop
            .with_label_values(&["set_chain_id"])
            .start_timer();
        for signed_order_with_metadata in &mut signed_orders_with_metadatas {
            // Set the chain_ids
            signed_order_with_metadata.signed_order.order.chain_id = self.chain_id.as_u64();
        }
        drop(step_timer);

        let step_timer = STEP_DURATION // Observes on drop
            .with_label_values(&["check_order_hash"])
            .start_timer();
        for signed_order_with_metadata in &mut signed_orders_with_metadatas {
            // Check order hash
            let valid_hash = signed_order_with_metadata.metadata.hash
                == signed_order_with_metadata.signed_order.hash();
            if !valid_hash {
                return Err(anyhow!(
                    "invalid order received from database, hash mismatch. (Are you connected to \
                     the right chain?)."
                ));
            }
        }
        drop(step_timer);

        let step_timer = STEP_DURATION // Observes on drop
            .with_label_values(&["sanity_check"])
            .start_timer();
        for signed_order_with_metadata in &mut signed_orders_with_metadatas {
            // Sanity check orders
            signed_order_with_metadata
                .signed_order
                .validate(chain)
                .with_context(|| {
                    format!(
                        "invalid order received from database. order: {}",
                        serde_json::to_string_pretty(signed_order_with_metadata)
                            .unwrap_or_else(|e| e.to_string())
                    )
                })?;
        }
        drop(step_timer);
        Ok(signed_orders_with_metadatas)
    }

    #[allow(clippy::large_types_passed_by_value)]
    pub async fn insert_order(
        &self,
        signed_order_with_metadata: SignedOrderWithMetadata,
    ) -> AnyResult<()> {
        OPS_COUNTER.with_label_values(&["insert_order"]).inc();
        trace!(order_hash = ?signed_order_with_metadata.metadata.hash, "Inserting order in database");
        // TODO: Validate order
        self.with_connection(move |connection| {
            use signed_orders_v4::{
                created_at, expiry, fee_recipient, hash, maker, maker_amount, maker_token, pool,
                remaining_fillable_taker_amount, salt, sender, signature, taker, taker_amount,
                taker_token, taker_token_fee_amount, verifying_contract,
            };

            let signed_order = signed_order_with_metadata.signed_order;
            let order = signed_order.order;
            let metadata = signed_order_with_metadata.metadata;

            let query = insert_into(signed_orders_v4::table)
                .values((
                    hash.eq(format!("{:?}", metadata.hash)),
                    maker_token.eq(format!("{:?}", order.maker_token)),
                    taker_token.eq(format!("{:?}", order.taker_token)),
                    maker_amount.eq(format!("{:?}", order.maker_amount)),
                    taker_amount.eq(format!("{:?}", order.taker_amount)),
                    maker.eq(format!("{:?}", order.maker)),
                    taker.eq(format!("{:?}", order.taker)),
                    pool.eq(format!("{:?}", order.pool)),
                    expiry.eq(format!("{:?}", order.expiry)),
                    salt.eq(format!("{:?}", order.salt)),
                    verifying_contract.eq(format!("{:?}", order.verifying_contract)),
                    taker_token_fee_amount.eq(format!("{:?}", order.taker_token_fee_amount)),
                    sender.eq(format!("{:?}", order.sender)),
                    fee_recipient.eq(format!("{:?}", order.fee_recipient)),
                    signature.eq(concatenate(&signed_order.signature)),
                    remaining_fillable_taker_amount.eq(format!("{:?}", metadata.remaining)),
                    created_at.eq(metadata.created_at),
                ))
                .on_conflict(hash)
                .do_update()
                .set(remaining_fillable_taker_amount.eq(format!("{:?}", metadata.remaining)));
            trace!(query = %debug_query::<Pg, _>(&query), "insert_order query");
            query.execute(connection)?;
            Ok(())
        })
        .await
        .context("error in insert_order query")
    }

    pub async fn update_order(&self, order_hash: H256, remaining: U128) -> AnyResult<()> {
        OPS_COUNTER.with_label_values(&["update_order"]).inc();
        trace!(?order_hash, ?remaining, "Updating order in database");
        self.with_connection(move |connection| {
            use signed_orders_v4::{hash, invalid_since, remaining_fillable_taker_amount, table};

            let query = update(table.filter(hash.eq(format!("{:?}", order_hash)))).set((
                remaining_fillable_taker_amount.eq(remaining.to_string()),
                invalid_since.eq(Option::<i64>::None),
            ));
            trace!(query = %debug_query::<Pg, _>(&query), "update_order query");
            query.execute(connection)?;
            Ok(())
        })
        .await
        .context("error in update_order query")
    }

    pub async fn invalidate_order(&self, order_hash: H256, block_number: U64) -> AnyResult<()> {
        OPS_COUNTER.with_label_values(&["invalidate_order"]).inc();
        trace!(?order_hash, ?block_number, "Marking order as invalid");
        self.with_connection(move |connection| {
            use signed_orders_v4::{hash, invalid_since, table};

            let signed_block_number = i64::try_from(block_number).unwrap();

            let was_valid_in_an_earlier_block = invalid_since
                .is_null()
                .or(invalid_since.gt(signed_block_number));
            let query = update(
                table.filter(
                    hash.eq(format!("{:?}", order_hash))
                        .and(was_valid_in_an_earlier_block),
                ),
            )
            .set(invalid_since.eq(signed_block_number));
            trace!(query = %debug_query::<Pg, _>(&query), "invalidate_order query");
            let count_updated = query.execute(connection)?;
            info!("{} order(s) marked as invalid", count_updated);
            Ok(())
        })
        .await
        .context("error in invalidate_order query")
    }

    pub async fn delete_orders(&self, block_number: U64) -> AnyResult<()> {
        OPS_COUNTER.with_label_values(&["delete_orders"]).inc();
        trace!(
            ?block_number,
            "Deleting orders invalid since block (or before) from database"
        );
        self.with_connection(move |connection| {
            use signed_orders_v4::{invalid_since, table};
            let signed_block_number = i64::try_from(block_number).unwrap();
            let query = delete(table.filter(invalid_since.le(signed_block_number)));
            trace!(query = %debug_query::<Pg, _>(&query), "delete_orders query");
            let count_deleted = query.execute(connection)?;
            info!("{} invalid order(s) deleted", count_deleted);
            Ok(())
        })
        .await
        .context("error in delete_orders query")
    }

    /// Execute a blocking operation using the [`PgConnection`] asynchronously
    /// in a worker thread and collect any errors or panics.
    async fn with_connection<F, T>(&self, f: F) -> AnyResult<T>
    where
        F: FnOnce(&PgConnection) -> AnyResult<T> + Send + 'static,
        T: Send + 'static,
    {
        let _timer = LATENCY.start_timer(); // Observes on drop
        let connection = self.connection.clone();
        spawn_blocking(move || {
            let lock = connection
                .lock()
                .map_err(|_| anyhow!("database lock was poisoned"))?;
            f(&lock)
        })
        .await
        .any_flatten()
    }
}

fn concatenate(signature: &Signature) -> String {
    vec![
        u32::from(signature.signature_type).to_string(),
        format!("{:?}", signature.r),
        format!("{:?}", signature.s),
        format!("{:?}", signature.v),
    ]
    .join(",")
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[tokio::test]
    #[ignore]
    #[allow(clippy::semicolon_if_nothing_returned)] // False positive
    async fn test_db() {
        let options = Options {
            database: Url::parse("postgres://postgres:postgres@localhost/diva-api").unwrap(),
        };
        let chain_id = U256::one();
        let db = Database::connect(options, chain_id).await.unwrap();

        let signed_orders_with_metadata = db.get_orders(&ChainInfo::default()).await.unwrap();
        let signed_order = &signed_orders_with_metadata[0].signed_order;

        db.invalidate_order(signed_order.order.hash(), 10.into())
            .await
            .unwrap();
        db.delete_orders(10.into()).await.unwrap();

        db.insert_order(signed_orders_with_metadata[0])
            .await
            .unwrap();

        // let new_order: Vec<SignedOrder> = table
        //     .filter(hash.eq(format!("{:?}", order.order.hash())))
        //     .load(&conn)
        //     .unwrap();
        // assert_eq!(new_order[0], order);
    }
}
