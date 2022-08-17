// TODO(mason): include link to SRA's schema.

table! {
    signed_orders_v4 (hash) {
        hash -> Varchar,
        maker_token -> Varchar,
        taker_token -> Varchar,
        maker_amount -> Varchar,
        taker_amount -> Varchar,
        maker -> Varchar,
        taker -> Varchar,
        pool -> Varchar,
        expiry -> Varchar,
        salt -> Varchar,
        verifying_contract -> Varchar,
        taker_token_fee_amount -> Varchar,
        sender -> Varchar,
        fee_recipient -> Varchar,
        signature -> Varchar,
        remaining_fillable_taker_amount -> Varchar,
        created_at -> Timestamptz,
        invalid_since -> Nullable<BigInt>,
    }
}
