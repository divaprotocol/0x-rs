use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("web3 initialization error")]
    Web3(#[from] web3::Error),
    #[error("Contract query error")]
    Contract(#[from] web3::contract::Error),
    #[error("ABI encoding error")]
    Abi(#[from] ethabi::Error),
}
