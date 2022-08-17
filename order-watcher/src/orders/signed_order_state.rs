use serde::{Deserialize, Serialize};
use types::{proto::zeroex::metadata::OrderStatus as OrderStatusProto, FromProto, IntoProto};
use web3::types::{H256, U128};

use super::Error;
use crate::require;

// TODO: just use the proto enum instead.
/// See <https://protocol.0x.org/en/latest/basics/functions.html#getlimitorderinfo>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    Added,
    Invalid,
    Fillable,
    FullyFilled,
    Cancelled,
    Expired,
}

impl FromProto for OrderStatus {
    type Proto = OrderStatusProto;

    fn from_proto(p: Self::Proto) -> Self {
        match p {
            OrderStatusProto::Added => Self::Added,
            OrderStatusProto::Invalid => Self::Invalid,
            OrderStatusProto::Fillable => Self::Fillable,
            OrderStatusProto::FullyFilled => Self::FullyFilled,
            OrderStatusProto::Cancelled => Self::Cancelled,
            OrderStatusProto::Expired => Self::Expired,
        }
    }
}

impl IntoProto for OrderStatus {
    type Proto = OrderStatusProto;

    fn into_proto(self) -> Self::Proto {
        match self {
            Self::Added => OrderStatusProto::Added,
            Self::Invalid => OrderStatusProto::Invalid,
            Self::Fillable => OrderStatusProto::Fillable,
            Self::FullyFilled => OrderStatusProto::FullyFilled,
            Self::Cancelled => OrderStatusProto::Cancelled,
            Self::Expired => OrderStatusProto::Expired,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedOrderState {
    pub hash: H256,
    pub status: OrderStatus,
    pub taker_asset_filled_amount: U128,
    pub taker_asset_fillable_amount: U128,
    pub is_signature_valid: bool,
}

impl SignedOrderState {
    pub const fn validate(&self) -> Result<(), Error> {
        require!(self.is_signature_valid, Error::InvalidSignature);
        match self.status {
            OrderStatus::Added | OrderStatus::Fillable => Ok(()),
            OrderStatus::Invalid => Err(Error::Unfunded),
            OrderStatus::FullyFilled => Err(Error::FullyFilled),
            OrderStatus::Cancelled => Err(Error::Cancelled),
            OrderStatus::Expired => Err(Error::Expired),
        }
    }
}
