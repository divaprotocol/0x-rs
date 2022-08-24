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
    /// Ethereum connection string.
    #[structopt(
        short,
        long,
        env = "ETHEREUM",
        default_value = "https://goerli.infura.io/v3/238bdc81208d4476bd300eb2fdfc9da5"
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
        info!("Connecting to Ethereum at {}", options.ethereum);
        let transport = Http::new(options.ethereum.as_str())?;
        let web3 = Web3::new(transport);

        // Verify chain id
        let chain_id = web3.eth().chain_id().await?;
        let chain = ChainInfo {
            chain_id,
            exchange: options.exchange,
            flash_wallet: options.flash_wallet,
            block_timeout: BLOCK_TIMEOUT,
            request_timeout: REQUEST_TIMEOUT,
            max_reorg: options.max_reorg,
        };
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
