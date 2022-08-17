//! Abstraction type over large event messages.
//!
//! *Note*: The implementation is handwritten instead of generated from proto
//! files so that it is generic.
//!
//! ## To do
//!
//! * Copy free implementation maintaining only a reference to the object on
//!   encoding.

use prost::{Message, Oneof};

const LARGE_TRESHOLD: usize = 1_000_000;

// Message also implements Debug + Default
#[derive(Clone, PartialEq, Message)]
pub struct MaybeLarge<T>
where
    T: Message + Default + Send + Sync,
{
    #[prost(oneof = "MaybeLargeEnum", tags = "1, 2")]
    pub maybe_large: Option<MaybeLargeEnum<T>>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum MaybeLargeEnum<T: Message + Default + Send + Sync>
where
    T: Message + Default + Send + Sync,
{
    #[prost(message, tag = "1")]
    Large(Large),
    #[prost(message, tag = "2")]
    Event(T),
}

#[derive(Clone, PartialEq, Message)]
pub struct Large {
    /// Path relative to the base URI containing a `Batch` object.
    #[prost(string, tag = "1")]
    payload_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;
}
