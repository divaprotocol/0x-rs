use std::convert::TryInto;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use types::{
    proto::zeroex::{
        LimitOrder as LimitOrderProto, Metadata as MetadataProto, OrderEvent,
        Signature as SignatureProto,
    },
    FromProto, IntoProto,
};
use web3::types::{Address, H256, U128, U256};

use super::{LimitOrder, Metadata, OrderStatus, Signature, SignedOrder};
use crate::orders::SignatureType;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
pub struct SignedOrderWithMetadata {
    #[serde(rename = "order")]
    pub signed_order: SignedOrder,
    #[serde(rename = "metaData")] // sic
    pub metadata:     Metadata,
}

impl FromProto for SignedOrderWithMetadata {
    type Proto = OrderEvent;

    fn from_proto(p: Self::Proto) -> Self {
        let limit_order = p.limit_order.unwrap();
        let metadata = p.metadata.unwrap();
        let signature = p.signature.unwrap();

        let created_at = metadata.created_at.unwrap();

        Self {
            signed_order: SignedOrder {
                order:     LimitOrder {
                    maker:                  limit_order.maker.map(Address::from_proto).unwrap(),
                    taker:                  limit_order.taker.map(Address::from_proto).unwrap(),
                    maker_token:            limit_order
                        .maker_token
                        .map(Address::from_proto)
                        .unwrap(),
                    taker_token:            limit_order
                        .taker_token
                        .map(Address::from_proto)
                        .unwrap(),
                    maker_amount:           limit_order.maker_amount.map(U128::from_proto).unwrap(),
                    taker_amount:           limit_order.taker_amount.map(U128::from_proto).unwrap(),
                    expiry:                 limit_order.expiry,
                    salt:                   limit_order.salt.map(U256::from_proto).unwrap(),
                    fee_recipient:          limit_order
                        .fee_recipient
                        .map(Address::from_proto)
                        .unwrap(),
                    pool:                   limit_order.pool.map(H256::from_proto).unwrap(),
                    sender:                 limit_order.sender.map(Address::from_proto).unwrap(),
                    verifying_contract:     limit_order
                        .verifying_contract
                        .map(Address::from_proto)
                        .unwrap(),
                    taker_token_fee_amount: limit_order
                        .taker_token_fee_amount
                        .map(U128::from_proto)
                        .unwrap(),
                    chain_id:               limit_order.chain_id,
                },
                signature: Signature {
                    r:              signature.r.map(H256::from_proto).unwrap(),
                    s:              signature.s.map(H256::from_proto).unwrap(),
                    v:              signature.v.try_into().unwrap(),
                    signature_type: SignatureType::from_proto(
                        types::proto::zeroex::signature::Type::from_i32(signature.r#type).unwrap(),
                    ),
                },
            },
            metadata:     Metadata {
                hash:       H256::from_proto(metadata.hash.unwrap()),
                created_at: DateTime::<Utc>::from_utc(
                    NaiveDateTime::from_timestamp(
                        created_at.seconds,
                        created_at.nanos.try_into().unwrap(),
                    ),
                    Utc,
                ),
                remaining:  U128::from_proto(metadata.remaining.unwrap()),
                status:     OrderStatus::from_proto(
                    types::proto::zeroex::metadata::OrderStatus::from_i32(metadata.order_status)
                        .unwrap(),
                ),
            },
        }
    }
}

impl IntoProto for SignedOrderWithMetadata {
    type Proto = OrderEvent;

    fn into_proto(self) -> Self::Proto {
        let limit_order = self.signed_order.order;
        let limit_order = LimitOrderProto {
            maker:                  Some(limit_order.maker.into_proto()),
            taker:                  Some(limit_order.taker.into_proto()),
            maker_token:            Some(limit_order.maker_token.into_proto()),
            taker_token:            Some(limit_order.taker_token.into_proto()),
            maker_amount:           Some(limit_order.maker_amount.into_proto()),
            taker_amount:           Some(limit_order.taker_amount.into_proto()),
            expiry:                 limit_order.expiry,
            salt:                   Some(limit_order.salt.into_proto()),
            fee_recipient:          Some(limit_order.fee_recipient.into_proto()),
            pool:                   Some(limit_order.pool.into_proto()),
            sender:                 Some(limit_order.sender.into_proto()),
            verifying_contract:     Some(limit_order.verifying_contract.into_proto()),
            taker_token_fee_amount: Some(limit_order.taker_token_fee_amount.into_proto()),
            chain_id:               limit_order.chain_id,
        };

        let metadata = self.metadata;
        let metadata_proto = MetadataProto {
            created_at:   Some(prost_types::Timestamp {
                seconds: metadata.created_at.timestamp(),
                nanos:   metadata
                    .created_at
                    .timestamp_subsec_nanos()
                    .try_into()
                    .unwrap(),
            }),
            hash:         Some(metadata.hash.into_proto()),
            order_status: metadata.status.into_proto().into(),
            remaining:    Some(metadata.remaining.into_proto()),
        };

        let signature = self.signed_order.signature;
        let signature_proto = SignatureProto {
            r:      Some(signature.r.into_proto()),
            s:      Some(signature.s.into_proto()),
            v:      signature.v.into(),
            r#type: signature.signature_type.into_proto().into(),
        };

        OrderEvent {
            limit_order: Some(limit_order),
            metadata:    Some(metadata_proto),
            signature:   Some(signature_proto),
        }
    }
}

#[cfg(test)]
pub mod test {
    use chrono::{DateTime, NaiveDateTime, Utc};
    use serde_json::{self, json};
    use web3::types::{H256, U128};

    use super::*;
    use crate::OrderStatus;

    #[test]
    fn test_decoding() {
        let order = SignedOrderWithMetadata {
            signed_order: SignedOrder::default(),
            metadata:     Metadata {
                hash:       H256::default(),
                remaining:  U128::default(),
                status:     OrderStatus::Fillable,
                created_at: DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(0, 0), Utc),
            },
        };

        let expected = json!({
            "order": {
                "makerToken": "0x0000000000000000000000000000000000000000",
                "takerToken": "0x0000000000000000000000000000000000000000",
                "makerAmount": "0",
                "takerAmount": "0",
                "maker": "0x0000000000000000000000000000000000000000",
                "taker": "0x0000000000000000000000000000000000000000",
                "pool": "0x0000000000000000000000000000000000000000000000000000000000000000",
                "expiry": "0",
                "salt": "0",
                "chainId": 0,
                "verifyingContract": "0x0000000000000000000000000000000000000000",
                "takerTokenFeeAmount": "0",
                "sender": "0x0000000000000000000000000000000000000000",
                "feeRecipient": "0x0000000000000000000000000000000000000000",
                "signature": {
                    "v": 0,
                    "r": "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "s": "0x0000000000000000000000000000000000000000000000000000000000000000",
                    "signatureType": 2
                }
            },
            "metaData": {
                "orderHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
                "remainingFillableTakerAmount": "0",
                "state": "FILLABLE",
                "createdAt": "1970-01-01T00:00:00Z"
            }
        });

        assert_eq!(serde_json::to_value(&order).unwrap(), expected);
    }
}
