use std::borrow::Cow;

use serde::{
    de::{Deserialize, Deserializer, Error},
    ser::Serializer,
};
use web3::types::{U128, U256};

fn try_hex(str: &str) -> Option<&str> {
    if str.len() >= 2 && (&str[..2] == "0x" || &str[..2] == "0X") {
        Some(&str[2..])
    } else {
        None
    }
}

/// Serialize using [`ToString`], which for numbers gives a decimal string.
pub fn to_string<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: ToString,
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

pub fn u64_from_str<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let str = <Cow<'de, str>>::deserialize(deserializer)?;
    try_hex(&str)
        .map_or_else(|| str.parse(), |hex| u64::from_str_radix(hex, 16))
        .map_err(D::Error::custom)
}

pub fn u128_from_str<'de, D: Deserializer<'de>>(deserializer: D) -> Result<U128, D::Error> {
    let str = <Cow<'de, str>>::deserialize(deserializer)?;
    try_hex(&str)
        .map_or_else(|| str.parse(), |hex| u128::from_str_radix(hex, 16))
        .map(u128::into)
        .map_err(D::Error::custom)
}

pub fn u256_from_str<'de, D: Deserializer<'de>>(deserializer: D) -> Result<U256, D::Error> {
    let str = <Cow<'de, str>>::deserialize(deserializer)?;
    try_hex(&str).map_or_else(
        || U256::from_dec_str(&str).map_err(D::Error::custom),
        |hex| U256::from_str_radix(hex, 16).map_err(D::Error::custom),
    )
}

pub mod u64_dec {
    pub use super::{to_string as serialize, u64_from_str as deserialize};
}

pub mod u128_dec {
    pub use super::{to_string as serialize, u128_from_str as deserialize};
}

pub mod u256_dec {
    pub use super::{to_string as serialize, u256_from_str as deserialize};
}
