use std::str::FromStr;

use chrono::{DateTime, Utc};
use diesel::Queryable;
use tracing::error;
use web3::types::{Address, H256, U128, U256};

use crate::{
    database::signed_orders_v4,
    orders::{LimitOrder, Metadata, OrderStatus, Signature, SignatureType, SignedOrder},
    SignedOrderWithMetadata,
};

/// Convert a database record to a [`SignedOrder`]
///
/// *Note* that the database does not store the [`LimitOrder::chain_id`]. This
/// field will be initialized with the default value (`0`).
impl Queryable<signed_orders_v4::SqlType, diesel::pg::Pg> for SignedOrderWithMetadata {
    #[allow(clippy::type_complexity)] // This is what a row looks like in the database.
    type Row = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        DateTime<Utc>,
        Option<i64>,
    );

    #[allow(clippy::similar_names)] // `maker` and `taker` are too similar.
    fn build(row: Self::Row) -> Self {
        let (
            hash,
            maker_token,
            taker_token,
            maker_amount,
            taker_amount,
            maker,
            taker,
            pool,
            expiry,
            salt,
            verifying_contract,
            taker_token_fee_amount,
            sender,
            fee_recipient,
            signature,
            remaining_fillable_taker_amount,
            created_at,
            invalid_since,
        ) = row;
        let order = LimitOrder {
            maker:                  parse_prefixed_address(&maker),
            taker:                  parse_prefixed_address(&taker),
            maker_token:            parse_prefixed_address(&maker_token),
            taker_token:            parse_prefixed_address(&taker_token),
            maker_amount:           parse_u128(&maker_amount),
            taker_amount:           parse_u128(&taker_amount),
            expiry:                 u64::from_str(&expiry).unwrap(),
            salt:                   parse_u256(&salt),
            fee_recipient:          parse_prefixed_address(&fee_recipient),
            pool:                   parse_prefixed_hash(&pool),
            taker_token_fee_amount: parse_u128(&taker_token_fee_amount),
            sender:                 parse_prefixed_address(&sender),
            verifying_contract:     parse_prefixed_address(&verifying_contract),
            chain_id:               u64::default(),
        };
        #[allow(clippy::single_match_else)] // TODO: Clean up and avoid alloc.
        let signature = match signature.split(',').collect::<Vec<_>>().as_slice() {
            [signature_type, r, s, v] => {
                Signature {
                    r:              parse_prefixed_hash(r),
                    s:              parse_prefixed_hash(s),
                    v:              u8::from_str(v).unwrap(),
                    signature_type: match u64::from_str(signature_type).unwrap() {
                        2 => SignatureType::EIP712,
                        3 => SignatureType::EthSign,
                        _ => panic!(),
                    },
                }
            }
            _ => {
                // Unfortunately Diesel does not allow returning errors.
                error!(?hash, "Invalid signature");
                std::process::abort(); // Terminate order-watcher (panic! would
                                       // only crash the thread)
            }
        };
        let metadata = Metadata {
            hash: parse_prefixed_hash(&hash),
            remaining: parse_u128(&remaining_fillable_taker_amount),
            status: if invalid_since.is_none() {
                OrderStatus::Fillable
            } else {
                OrderStatus::Invalid
            },
            created_at,
        };
        Self {
            signed_order: SignedOrder { order, signature },
            metadata,
        }
    }
}

fn parse_prefixed_address(s: &str) -> Address {
    if s.is_empty() {
        Address::default()
    } else {
        Address::from_str(&s[2..]).unwrap_or_else(|_| panic!("invalid string for address: {:?}", s))
    }
}

fn parse_prefixed_hash(s: &str) -> H256 {
    H256::from_str(&s[2..]).unwrap_or_else(|_| panic!("invalid hex string for H256: {:?}", s))
}

fn parse_u128(s: &str) -> U128 {
    U128::from_dec_str(s).unwrap_or_else(|_| panic!("invalid decimal string for U128: {:?}", s))
}

fn parse_u256(s: &str) -> U256 {
    U256::from_dec_str(s).unwrap_or_else(|_| panic!("invalid decimal string for U256: {:?}", s))
}
