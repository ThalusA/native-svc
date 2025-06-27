/* Internal connector errors */
use std::io;
use embedded_svc::io::{Error, ErrorKind};
use hyper::header::{InvalidHeaderName, InvalidHeaderValue};

#[derive(Debug)]
pub struct HyperError(io::Error);

impl From<hyper::Error> for HyperError {
    fn from(value: hyper::Error) -> Self {
        Self(io::Error::other(value))
    }
}

impl From<InvalidHeaderName> for HyperError {
    fn from(value: InvalidHeaderName) -> Self {
        Self(io::Error::other(value))
    }
}

impl From<InvalidHeaderValue> for HyperError {
    fn from(value: InvalidHeaderValue) -> Self {
        Self(io::Error::other(value))
    }
}

impl Error for HyperError {
    fn kind(&self) -> ErrorKind {
        ErrorKind::from(self.0.kind())
    }
}