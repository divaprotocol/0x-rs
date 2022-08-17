use core::f64;

use once_cell::sync::Lazy;
use prometheus::{
    exponential_buckets, linear_buckets, register_histogram, register_int_counter, Histogram,
    IntCounter,
};

pub static BLOCKS_RECEIVED: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "blocks_received",
        "Count of order state requests deduplicated."
    )
    .unwrap()
});

pub static CONNECTION_ATTEMPTS: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "connection_attempts",
        "Number of attempts to connect to the block header stream."
    )
    .unwrap()
});

pub static BLOCKS_REWOUND: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "blocks_rewound",
        "The depth of reorgs.",
        linear_buckets(1.0, 1.0, 10).unwrap()
    )
    .unwrap()
});

pub static BLOCKS_ADDED: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "blocks_added",
        "The number of blocks added in one event.",
        linear_buckets(1.0, 1.0, 20).unwrap()
    )
    .unwrap()
});

pub static BLOCK_TIME: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "block_time",
        "The time between receiving new block events (excluding processing time).",
        linear_buckets(1.0, 2.0, 20).unwrap()
    )
    .unwrap()
});

pub static BLOCK_HEADER_LATENCY: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "block_header_latency",
        "The latency to request block headers."
    )
    .unwrap()
});
pub static BLOCK_HEADER_AGE: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "block_header_age",
        "The age of blocks at reception.",
        exponential_buckets(1.0, f64::consts::SQRT_2, 20).unwrap()
    )
    .unwrap()
});
