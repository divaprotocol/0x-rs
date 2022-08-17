use anyhow::Error as AnyError;
use hyper::{header, header::HeaderValue, Body, Error as HttpError, Response, StatusCode};
use serde_json::{json, Error as JsonError, Value as JsonValue};
use thiserror::Error;

use super::CONTENT_JSON;
use crate::orders;

/// See <https://0x.org/docs/api#error-reporting-format>
#[derive(Error, Debug)]
#[allow(dead_code, clippy::module_name_repetitions)]
pub enum ValidationError {
    #[error("Required field")]
    RequiredField,
    #[error("Incorrect format")]
    IncorrectFormat,
    #[error("{:?}", orders::Error::InvalidVerifyingContract)]
    InvalidAddress,
    #[error("Address not supported")]
    AddressNotSupported,
    #[error("Value out of range")]
    OutOfRange,
    #[error("{:?}", orders::Error::InvalidSignature)]
    InvalidSignature,
    #[error("Unsupported option")]
    UnsupportedOption,
    #[error("{0}")]
    InvalidOrder(orders::Error),
    #[error("Internal error")]
    InternalError,
    #[error("Token is not supported")]
    UnsupportedToken,
    #[error("Invalid field")]
    InvalidField,
}

impl ValidationError {
    pub const fn error_code(&self) -> u32 {
        #[allow(clippy::enum_glob_use)]
        use ValidationError::*;
        match self {
            RequiredField => 1000,
            IncorrectFormat => 1001,
            InvalidAddress => 1002,
            AddressNotSupported => 1003,
            OutOfRange => 1004,
            InvalidSignature => 1005,
            UnsupportedOption => 1006,
            InvalidOrder(_) => 1007,
            InternalError => 1008,
            UnsupportedToken => 1009,
            InvalidField => 1010,
        }
    }

    // This exists to maintain backwards compatibility with the errors SRA generated
    // when it was using Mesh for validation.
    pub fn to_json(&self, i: usize) -> JsonValue {
        json!({
            "code": self.error_code(),
            "reason": format!("{}", self),
            "field": format!("signedOrder[{}]", i),
        })
    }
}

impl From<orders::Error> for ValidationError {
    fn from(e: orders::Error) -> Self {
        match e {
            orders::Error::ZeroMakerAmount
            | orders::Error::ZeroTakerAmount
            | orders::Error::InvalidMakerAddress
            | orders::Error::InvalidTakerAddress
            | orders::Error::Cancelled
            | orders::Error::Expired
            | orders::Error::Unfunded
            | orders::Error::FullyFilled => Self::InvalidOrder(e),
            orders::Error::InvalidSignature => Self::InvalidSignature,
            orders::Error::InvalidVerifyingContract => Self::InternalError,
        }
    }
}

#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum Error {
    #[error("error in http stream")]
    Http(#[from] HttpError),
    #[error("invalid json")]
    Json(#[from] JsonError),
    #[error("invalid http method, expected POST")]
    InvalidMethod,
    #[error("not found")]
    NotFound,
    #[error("invalid content type, expecting \"application/json\"")]
    InvalidContentType,
    #[error("internal error when validating orders")]
    InternalError,
    #[error("Validation failed")]
    OrderInvalid(Vec<ValidationError>),
}

impl Error {
    /// Create error response
    /// See <https://0x.org/docs/api#errors>
    pub fn into_response(self) -> Response<Body> {
        let (code, status_code) = match self {
            Error::InvalidMethod => (405, StatusCode::METHOD_NOT_ALLOWED),
            Error::NotFound => (404, StatusCode::NOT_FOUND),
            Error::Json(_) => (101, StatusCode::BAD_REQUEST),
            Error::OrderInvalid(_) => (100, StatusCode::BAD_REQUEST),
            _ => (400, StatusCode::BAD_REQUEST),
        };
        let validation = if let Error::OrderInvalid(validation) = &self {
            JsonValue::Array(
                validation
                    .iter()
                    .enumerate()
                    .map(|(i, e)| e.to_json(i))
                    .collect(),
            )
        } else {
            json!([])
        };
        let reason = format!("{:?}", AnyError::from(self));
        let json = json!({
            "code": code,
            "reason": reason,
            "validationErrors": validation
        });
        let json_str = serde_json::to_string_pretty(&json).unwrap_or_default();
        let mut response = Response::new(Body::from(json_str));
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_JSON));
        *response.status_mut() = status_code;
        response
    }
}
