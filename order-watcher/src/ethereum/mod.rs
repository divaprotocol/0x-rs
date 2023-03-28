mod abi_coding;
mod batcher;
mod chain_info;
mod error;

use core::time::Duration;

use anyhow::Result as AnyResult;
use structopt::StructOpt;
use tracing::info;
use url::Url;
use web3::{contract::Contract, transports::Http, types::Address, Web3};
use web3::types::{U256};
use dotenv::dotenv;
use std::env;
use konst::{primitive::parse_usize, result::unwrap_ctx};

use self::{
    abi_coding::{Input, Output},
    batcher::Batcher,
};
pub use self::{chain_info::ChainInfo, error::Error};

const BLOCK_TIMEOUT: Duration = Duration::from_secs(300);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const EXCHANGE_ABI: &[u8] = include_bytes!("../../ethereum-abis/exchange.json");

#[derive(Debug, PartialEq, StructOpt)]
pub struct Options {
    // Ethereum connection string.
    #[structopt(
        short,
        long,
        env = "ETHEREUM",
        default_value = "https://mainnet.infura.io/v3/"
    )]
    pub ethereum: Url,

    /// Exchange contract address.
    #[structopt(
        long,
        env = "EXCHANGE",
        default_value = "0xDef1C0ded9bec7F1a1670819833240f027b25EfF"
    )]
    pub exchange: Address,

    /// Flash wallet address. Only used to validate orders.
    #[structopt(
        long,
        env = "FLASH_WALLET",
        default_value = "0x22F9dCF4647084d6C31b2765F6910cd85C178C18"
    )]
    pub flash_wallet: Address,

    /// Maximum batch size for fetching order state
    #[structopt(long, env = "BATCH_SIZE", default_value = "512")]
    pub batch_size: usize,

    /// Maximum concurrent order state fetch requests
    #[structopt(long, env = "CONCURRENT", default_value = "16")]
    pub concurrent: usize,

    /// Maximum chain reorg depth that will be handled
    #[structopt(long, env = "MAX_REORG", default_value = "10")]
    pub max_reorg: usize,
}

#[derive(Clone, Debug)]
pub struct Ethereum {
    pub chain:    ChainInfo,
    pub web3:     Web3<Http>,
    pub exchange: Contract<Http>,
    pub batcher:  Batcher,
}

impl Ethereum {
    #[allow(clippy::similar_names)] // Watcher and Batcher are similar
    pub async fn connect(options: Options) -> AnyResult<Self> {
        dotenv().ok();
        // Verify chain id
        let chain_id = env::var("CHAIN_ID").unwrap();

        let mainnet_rpc_url = env::var("HTTPS_MAINNET_RPC_URL").unwrap();
        let goerli_rpc_url = env::var("HTTPS_GOERLI_RPC_URL").unwrap();
        let polygon_rpc_url = env::var("HTTPS_POLYGON_RPC_URL").unwrap();
        let mumbai_rpc_url = env::var("HTTPS_MUMBAI_RPC_URL").unwrap();

        let mut rpc_url = options.ethereum;

        if chain_id == "5" {
            rpc_url = goerli_rpc_url.parse().unwrap();
        } else if chain_id == "137" {
            rpc_url = polygon_rpc_url.parse().unwrap();
        } else if chain_id == "80001" {
            rpc_url = mumbai_rpc_url.parse().unwrap();
        } else {
            rpc_url = mainnet_rpc_url.parse().unwrap();
        }

        info!("Connecting to Ethereum at {}", rpc_url);

        let transport = Http::new(rpc_url.as_str())?;
        let web3 = Web3::new(transport);

        // Verify chain id
        // let chain_id = web3.eth().chain_id().await?;
        let mut chain = ChainInfo {
            chain_id: U256::from(unwrap_ctx!(parse_usize(&chain_id))),
            exchange: options.exchange,
            flash_wallet: options.flash_wallet,
            block_timeout: BLOCK_TIMEOUT,
            request_timeout: REQUEST_TIMEOUT,
            max_reorg: options.max_reorg,
        };

        if chain_id == "5" {
            chain.exchange = "0xf91bb752490473b8342a3e964e855b9f9a2a668e"
                .parse()
                .unwrap();
            chain.flash_wallet = "0xf15469c80a1965f5f90be5651fcb6c6f3392b2a1"
                .parse()
                .unwrap();
        } else if chain_id == "137" {
            chain.exchange = "0xdef1c0ded9bec7f1a1670819833240f027b25eff"
                .parse()
                .unwrap();
            chain.flash_wallet = "0xdB6f1920A889355780aF7570773609Bd8Cb1f498"
                .parse()
                .unwrap();
        } else if chain_id == "80001" {
            chain.exchange = "0xf471d32cb40837bf24529fcf17418fc1a4807626"
                .parse()
                .unwrap();
            chain.flash_wallet = "0x64254Cf2F3AbD765BeE46f8445B76e2bB0aF5A2c"
                .parse()
                .unwrap();
        }

        info!("Connected to Ethereum with chain id {}", chain.chain_id);

        // Wrap contracts
        let exchange = Contract::from_json(web3.eth(), chain.exchange, EXCHANGE_ABI)?;

        // Start batcher
        let batcher = Batcher::new(exchange.clone(), options.batch_size, options.concurrent);

        Ok(Self {
            chain,
            web3,
            exchange,
            batcher,
        })
    }
}
