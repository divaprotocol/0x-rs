use strum_macros::IntoStaticStr;
use thiserror::Error;
use tracing::error;

// Error messages are from
// https://github.com/0xProject/0x-mesh/blob/master/zeroex/ordervalidator/order_validator.go#L95
#[derive(Debug, Error, IntoStaticStr)]
pub enum Error {
    #[error("ORDER_HAS_INVALID_MAKER_ASSET_AMOUNT: order makerAssetAmount cannot be 0")]
    ZeroMakerAmount,
    #[error("ORDER_HAS_INVALID_TAKER_ASSET_AMOUNT: order takerAssetAmount cannot be 0")]
    ZeroTakerAmount,
    #[error(
        "ORDER_HAS_INVALID_MAKER_ASSET_DATA: order makerAssetData must encode a supported \
         assetData type"
    )]
    InvalidMakerAddress,
    #[error(
        "ORDER_HAS_INVALID_TAKER_ASSET_DATA: order takerAssetData must encode a supported \
         assetData type"
    )]
    InvalidTakerAddress,
    #[error(
        "INCORRECT_EXCHANGE_ADDRESS: the exchange address for the order does not match the chain \
         ID/network ID"
    )]
    InvalidVerifyingContract,
    #[error("ORDER_HAS_INVALID_SIGNATURE: order signature must be valid")]
    InvalidSignature,
    #[error("ORDER_CANCELLED: order cancelled")]
    Cancelled,
    #[error("ORDER_EXPIRED: order expired according to latest block timestamp")]
    Expired,
    #[error(
        "ORDER_UNFUNDED: maker has insufficient balance or allowance for this order to be filled"
    )]
    Unfunded,
    #[error("ORDER_FULLY_FILLED: order already fully filled")]
    FullyFilled,
}
