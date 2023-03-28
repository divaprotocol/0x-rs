#![warn(clippy::all, clippy::pedantic, clippy::cargo, clippy::nursery)]

mod allocator;
mod logging;
mod prometheus;
mod shutdown;

use anyhow::{Context as _, Result as AnyResult};
use block_watcher::producer::Producer;
use structopt::StructOpt;
use tokio::{runtime, spawn, sync::oneshot};
use tracing::info;
use url::Url;
use dotenv::dotenv;
use std::env;

use self::{allocator::Allocator, logging::LogOptions};

const VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    "\n",
    env!("COMMIT_SHA"),
    " ",
    env!("COMMIT_DATE"),
    "\n",
    env!("TARGET"),
    " ",
    env!("BUILD_DATE"),
    "\n",
    env!("CARGO_PKG_AUTHORS"),
    "\n",
    env!("CARGO_PKG_HOMEPAGE"),
    "\n",
    env!("CARGO_PKG_DESCRIPTION"),
);

#[cfg(not(feature = "mimalloc"))]
#[global_allocator]
pub static ALLOCATOR: Allocator<allocator::StdAlloc> = allocator::new_std();

#[cfg(feature = "mimalloc")]
#[global_allocator]
pub static ALLOCATOR: Allocator<allocator::MiMalloc> = allocator::new_mimalloc();

#[derive(StructOpt)]
struct Options {
    #[structopt(flatten)]
    log:            LogOptions,
    #[structopt(flatten)]
    pub prometheus: prometheus::Options,
    #[structopt(flatten)]
    app:            types::Options,
    #[structopt(
        long,
        env = "BLOCK_WATCHER_TOPIC",
        default_value = "block_watcher_events"
    )]
    topic:          String,
    /// Ethereum connection string.
    #[structopt(
        short,
        long,
        env = "ETHEREUM",
        default_value = "wss://mainnet.infura.io/ws/v3/"
    )]
    pub ethereum:   Url,
}

fn main() -> AnyResult<()> {
    dotenv().ok();
    // Parse CLI and handle help and version (which will stop the application).
    let matches = Options::clap().long_version(VERSION).get_matches();
    let mut options = Options::from_clap(&matches);

    // Meter memory consumption
    ALLOCATOR.start_metering();

    // Start log system
    options.log.init()?;

    let chain_id = env::var("CHAIN_ID").unwrap();
    let goerli_rpc_url = env::var("GOERLI_RPC_URL").unwrap();
    let polygon_rpc_url = env::var("POLYGON_RPC_URL").unwrap();
    let mumbai_rpc_url = env::var("MUMBAI_RPC_URL").unwrap();

    if chain_id == "5" {
        options.ethereum = goerli_rpc_url.parse().unwrap();
    } else if chain_id == "137" {
        options.ethereum = polygon_rpc_url.parse().unwrap();
    } else if chain_id == "80001" {
        options.ethereum = mumbai_rpc_url.parse().unwrap();
    }

    // Launch Tokio runtime
    runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Error creating Tokio runtime")?
        .block_on(async {
            let prometheus = options.prometheus.clone();
            spawn(prometheus::main(prometheus));

            let (send, shutdown) = oneshot::channel();
            spawn(async {
                shutdown::signal_shutdown().await.unwrap();
                let _ = send.send(());
            });

            spawn(async {
                let producer = Producer::new(options.app, options.topic).await.unwrap();
                let _ = producer.start(options.ethereum).await;
            });

            shutdown.await
        })?;

    // Terminate successfully
    info!("program terminating normally");
    Ok(())
}

#[cfg(test)]
pub mod test {
    use tracing::{error, warn};
    use tracing_test::traced_test;

    use super::*;

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
