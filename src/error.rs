/* Internal connector errors */
use embedded_svc::io::{Error as SvcError, ErrorKind as SvcErrorKind};
use hyper::http;
use std::io;
use hyper::header::{InvalidHeaderName, InvalidHeaderValue};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HyperError {
    #[error("io error: {0:?}")]
    Io(io::Error),
    #[error("http error: {0:?}")]
    Http(http::Error),
    #[error("hyper error: {0:?}")]
    Hyper(hyper::Error),
    #[error("client error: {0:?}")]
    Client(hyper_util::client::legacy::Error),
    #[error("tokio runtime initialization error: {0:?}")]
    RuntimeCreation(io::Error),
    #[error("unsupported http method: {0:?}")]
    UnsupportedMethod(String),
    #[error("no response initialized")]
    NoResponse,
    #[error("no request initialized")]
    NoRequest,
    #[error("invalid header name: {0:?}")]
    InvalidHeaderName(InvalidHeaderName),
    #[error("invalid header value: {0:?}")]
    InvalidHeaderValue(InvalidHeaderValue),
}


impl SvcError for HyperError {
    fn kind(&self) -> SvcErrorKind {
        SvcErrorKind::Other
    }
}