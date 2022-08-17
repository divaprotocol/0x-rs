use core::convert::{TryFrom, TryInto};

use serde::{
    de::{Deserializer, Error},
    ser::Serializer,
    Deserialize, Serialize,
};
use thiserror::Error;
use types::{FromProto, IntoProto};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureType {
    EIP712,
    EthSign,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum SingatureCodeError {
    #[error("Unsupported signature type, expected 2 or 3")]
    Unsupported,
}

impl Default for SignatureType {
    fn default() -> Self {
        Self::EIP712
    }
}

impl FromProto for SignatureType {
    type Proto = types::proto::zeroex::signature::Type;

    fn from_proto(p: Self::Proto) -> Self {
        match p {
            types::proto::zeroex::signature::Type::Eip712 => Self::EIP712,
            types::proto::zeroex::signature::Type::EthSign => Self::EthSign,
        }
    }
}

impl IntoProto for SignatureType {
    type Proto = types::proto::zeroex::signature::Type;

    fn into_proto(self) -> Self::Proto {
        match self {
            Self::EIP712 => types::proto::zeroex::signature::Type::Eip712,
            Self::EthSign => types::proto::zeroex::signature::Type::EthSign,
        }
    }
}

impl From<SignatureType> for u32 {
    fn from(value: SignatureType) -> Self {
        // See <https://0x.org/docs/api#signed-order>
        match value {
            SignatureType::EIP712 => 2,
            SignatureType::EthSign => 3,
        }
    }
}

impl TryFrom<u32> for SignatureType {
    type Error = SingatureCodeError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            2 => Ok(Self::EIP712),
            3 => Ok(Self::EthSign),
            _ => Err(SingatureCodeError::Unsupported),
        }
    }
}

impl Serialize for SignatureType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32((*self).into())
    }
}

impl<'de> Deserialize<'de> for SignatureType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        u32::deserialize(deserializer)?
            .try_into()
            .map_err(D::Error::custom)
    }
}
