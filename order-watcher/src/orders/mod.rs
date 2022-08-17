mod error;
mod limit_order;
mod metadata;
mod signature_type;
mod signed_order;
mod signed_order_state;
mod signed_order_with_metadata;

pub use self::{
    error::Error,
    limit_order::LimitOrder,
    metadata::Metadata,
    signature_type::SignatureType,
    signed_order::{Signature, SignedOrder},
    signed_order_state::{OrderStatus, SignedOrderState},
    signed_order_with_metadata::SignedOrderWithMetadata,
};

#[cfg(feature = "bench")]
pub mod bench {
    use criterion::Criterion;

    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub fn group(criterion: &mut Criterion) {
        limit_order::bench::group(criterion);
        signed_order::bench::group(criterion);
    }
}
