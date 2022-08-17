use chrono::{offset::Utc, DateTime};
use serde::{Deserialize, Serialize};
use web3::types::{H256, U128};

use crate::{orders::OrderStatus, utils::serde::u128_dec};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    #[serde(rename = "orderHash")]
    pub hash:       H256,
    #[serde(rename = "remainingFillableTakerAmount", with = "u128_dec")]
    pub remaining:  U128,
    #[serde(rename = "state")]
    pub status:     OrderStatus,
    pub created_at: DateTime<Utc>,
}
