use core::time::Duration;

use web3::types::{Address, U256};

#[derive(Clone, PartialEq, Debug)]
pub struct ChainInfo {
    pub chain_id: U256,
    pub exchange: Address,

    // TODO: Instead of flash_wallet, should we have a set of blacklisted taker addresses instead?
    pub flash_wallet: Address,

    /// Maximum time to wait for the next block before it is considered a
    /// failure.
    pub block_timeout: Duration,

    /// Maximum time to wait for the an RPC request
    pub request_timeout: Duration,

    /// Max number of new blocks in a re-org
    pub max_reorg: usize,
}

/// Values for Ethereum main net
impl Default for ChainInfo {
    fn default() -> Self {
        Self {
            chain_id:        U256::one(),
            exchange:        "0xDef1C0ded9bec7F1a1670819833240f027b25EfF"
                .parse()
                .unwrap(),
            flash_wallet:    "0x22F9dCF4647084d6C31b2765F6910cd85C178C18"
                .parse()
                .unwrap(),
            block_timeout:   Duration::from_secs(300),
            request_timeout: Duration::from_secs(30),
            max_reorg:       10,
        }
    }
}
