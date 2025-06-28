//! Error types for the Hyper HTTP connection implementation.
//!
//! Defines a unified `HyperError` enum wrapping IO, HTTP, and `hyper` library errors,
//! as well as connector-specific conditions like missing requests/responses and unsupported methods.

use embedded_svc::io::{Error as SvcError, ErrorKind as SvcErrorKind};
use hyper::http;
use std::io;
use hyper::header::{InvalidHeaderName, InvalidHeaderValue};
use thiserror::Error;

/// A comprehensive error type for the Hyper-based HTTP client.
///
/// Wraps errors from various layers:
///
/// - `io::Error` for general I/O failures.
/// - `http::Error` for request building mistakes.
/// - `hyper::Error` for runtime or protocol-level failures.
/// - `hyper_util::client::legacy::Error` for client connector issues.
///
/// Also includes variants for unsupported HTTP methods, missing requests or responses,
/// and invalid header names or values.
#[derive(Error, Debug)]
pub enum HyperError {
    /// Underlying I/O error.
    #[error("io error: {0:?}")]
    Io(#[from] io::Error),

    /// Error constructing or parsing an HTTP message.
    #[error("http error: {0:?}")]
    Http(#[from] http::Error),

    /// Error returned by the Hyper library during request/response processing.
    #[error("hyper error: {0:?}")]
    Hyper(#[from] hyper::Error),

    /// Error originating from the Hyper legacy client connector.
    #[error("client error: {0:?}")]
    Client(#[from] hyper_util::client::legacy::Error),

    /// Failed to initialize the Tokio runtime.
    #[error("tokio runtime initialization error: {0:?}")]
    RuntimeCreation(io::Error),

    /// The connector did not support an HTTP method.
    #[error("unsupported http method: {0}")]
    UnsupportedMethod(String),

    /// No HTTP response has been received when expected.
    #[error("no response initialized")]
    NoResponse,

    /// No HTTP request has been initiated when expected.
    #[error("no request initialized")]
    NoRequest,

    /// A header name provided was invalid, according to HTTP specifications.
    #[error("invalid header name: {0:?}")]
    InvalidHeaderName(#[from] InvalidHeaderName),

    /// A header value provided was invalid according to HTTP value rules.
    #[error("invalid header value: {0:?}")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),
}

impl SvcError for HyperError {
    /// Maps all `HyperError` variants to `ErrorKind::Other` for embedded-svc.
    fn kind(&self) -> SvcErrorKind {
        SvcErrorKind::Other
    }
}
