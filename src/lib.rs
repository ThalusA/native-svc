pub mod error;

use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use crate::error::HyperError;
use embedded_svc::http::client::Connection;
use embedded_svc::http::{Headers, Method, Status};
use embedded_svc::io::{ErrorType, Read, Write};
use hyper::body::{Body, Bytes, Incoming};
use hyper::client::conn::http1::{handshake, Connection as HyperConnection, SendRequest};
use hyper::{HeaderMap, Request, Response, Uri};
use hyper::header::{HeaderName, HeaderValue};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio::runtime::Runtime;

pub struct HyperHttpConnection {
    rt: Runtime,
    hyper_connection: Option<HyperConnection<TokioIo<TcpStream>, Incoming>>,
    sender: Option<SendRequest<Incoming>>,
    request: Option<Request<Incoming>>,
    response: Option<Response<Incoming>>,
    pending_read: Vec<u8>
}

impl HyperHttpConnection {
    pub fn new() -> Self {
        Self { rt: Runtime::new().expect("failed to start tokio runtime"), hyper_connection: None, sender: None, request: None, response: None, pending_read: Default::default() }
    }

    pub fn connect(&mut self, url: Uri) {
        let host = url.host().expect("uri has no host");
        let port = url.port_u16().unwrap_or(80);
        let address = format!("{}:{}", host, port);
        let stream = self.rt.block_on(TcpStream::connect(address)).unwrap();
        let stream = TokioIo::new(stream);
        let (sender, conn) = self.rt.block_on(handshake(stream)).unwrap();
        self.hyper_connection = Some(conn);
        self.sender = Some(sender);
    }

    pub fn mut_hyper_connection(&mut self) -> &mut HyperConnection<TokioIo<TcpStream>, Incoming> {
        self.hyper_connection.as_mut().expect("should have a connection established")
    }

    pub fn sender_mut(&mut self) -> &mut SendRequest<Incoming> {
        self.sender.as_mut().expect("should have a connection established")
    }

    pub fn mut_request(&mut self) -> &mut Request<Incoming> {
        self.request.as_mut().expect("should be a request")
    }

    pub fn mut_response(&mut self) -> &mut Response<Incoming> {
        self.response.as_mut().expect("should be a response")
    }

    pub fn hyper_connection(&self) -> &HyperConnection<TokioIo<TcpStream>, Incoming> {
        self.hyper_connection.as_ref().expect("should have a connection established")
    }

    pub fn sender(&self) -> &SendRequest<Incoming> {
        self.sender.as_ref().expect("should have a connection established")
    }

    pub fn request(&self) -> &Request<Incoming> {
        self.request.as_ref().expect("should be a request")
    }

    pub fn response(&self) -> &Response<Incoming> {
        self.response.as_ref().expect("should be a response")
    }
}


impl ErrorType for HyperHttpConnection {
    type Error = HyperError;
}

impl Status for HyperHttpConnection {
    fn status(&self) -> u16 {
        self.response().status().as_u16()
    }

    fn status_message(&self) -> Option<&'_ str> {
        self.response().status().canonical_reason()
    }
}

impl Headers for HyperHttpConnection {
    fn header(&self, name: &str) -> Option<&'_ str> {
        self.response().headers().get(name).and_then(|value| value.to_str().ok())
    }
}

impl Read for HyperHttpConnection {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        if !self.pending_read.is_empty() {
            let length = self.pending_read.len().min(buffer.len());
            buffer[..length].copy_from_slice(&self.pending_read[..length]);
            self.pending_read.copy_from_slice(&buffer[length..]);
            return Ok(length);
        }
        let body = Pin::new(self.mut_response().body_mut());
        let mut cx = Context::from_waker(&Waker::noop());
        let frame = body.poll_frame(&mut cx).map_ok(|frame| frame.into_data());
        let body = match frame {
            Poll::Ready(Some(Ok(Ok(bytes)))) => bytes,
            _ => return Ok(0)
        };
        let length = body.len().min(buffer.len());
        buffer[..length].copy_from_slice(&body[..length]);
        self.pending_read.copy_from_slice(&buffer[length..]);
        Ok(length)
    }
}

impl Write for HyperHttpConnection {
    fn write(&mut self, buf: &[u8]) -> Result<usize, HyperError> {
        let body = self.mut_response().body_mut();
        Incoming::
        let mut buffer = vec![];
        buffer.extend_from_slice(&body[..]);
        buffer.extend_from_slice(buf);
        *body = Bytes::from(buffer);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), HyperError> {
        Ok(())
    }
}

impl Connection for HyperHttpConnection {
    type Headers = Self;
    type Read = Self;
    type RawConnectionError = HyperError;
    type RawConnection = Self;

    fn initiate_request<'a>(&'a mut self, method: Method, uri: &'a str, headers: &'a [(&'a str, &'a str)]) -> Result<(), Self::Error> {
        let mapped_method = match method {
            Method::Delete => hyper::Method::DELETE,
            Method::Get => hyper::Method::GET,
            Method::Head => hyper::Method::HEAD,
            Method::Post => hyper::Method::POST,
            Method::Put => hyper::Method::PUT,
            Method::Connect => hyper::Method::CONNECT,
            Method::Options => hyper::Method::OPTIONS,
            Method::Trace => hyper::Method::TRACE,
            Method::Patch => hyper::Method::PATCH,
            method => panic!("Method {method:?} is not supported"),
        };
        let mut header_map = HeaderMap::new();
        for &(name, value) in headers {
            let header_name = HeaderName::from_bytes(name.as_bytes()).map_err(HyperError::from)?;
            let header_value = HeaderValue::from_str(value).map_err(HyperError::from)?;
            header_map.insert(header_name, header_value);
        }

        let mut request = Request::builder().method(mapped_method).uri(uri);
        let _ = request.headers_mut().insert(&mut header_map);
        let request = request.body(Bytes::new()).unwrap();
        self.request = Some(request);
        self.response = None;
        Ok(())
    }

    fn is_request_initiated(&self) -> bool {
        self.request.is_some()
    }

    fn initiate_response(&mut self) -> Result<(), Self::Error> {
        if let Some(request) = self.request.take() {
            let response = self.sender().send_request(request).map_err(HyperError::from)?;
            self.response = Some(response);
            Ok(())
        } else {
            panic!("should have a request already initiated")
        }
    }

    fn is_response_initiated(&self) -> bool {
        self.response.is_some()
    }

    fn split(&mut self) -> (&Self::Headers, &mut Self::Read) {
        let headers: *const HyperHttpConnection = self as *const _;
        let headers = unsafe { headers.as_ref().unwrap() };

        (headers, self)
    }

    fn raw_connection(&mut self) -> Result<&mut Self::RawConnection, Self::Error> {
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_and_response_flow() {
        let client = Client::new();
        let mut conn = HyperHttpConnection {
            client,
            request: None,
            response: None,
        };

        let headers = &[("User-Agent", "TestAgent")];
        conn.initiate_request(Method::Get, "https://httpbin.org/get", headers).unwrap();
        assert!(conn.is_request_initiated());

        conn.initiate_response().unwrap();
        assert!(conn.is_response_initiated());

        let (hdrs, rdr) = conn.split();
        assert!(hdrs.status() >= 200);
        assert!(hdrs.header("content-type").is_some());

        let mut buf = [0; 4096];
        rdr.read(&mut buf).unwrap();
        unsafe { println!("{}", str::from_utf8_unchecked(&buf)); }
    }

    #[test]
    fn test_write_body_and_send() {
        let client = reqwest::blocking::Client::new();
        let mut conn = HyperHttpConnection {
            client,
            request: None,
            response: None,
        };

        conn.initiate_request(Method::Post, "https://httpbin.org/post", &[]).unwrap();
        conn.write(b"test body").unwrap();
        conn.flush().unwrap();

        conn.initiate_response().unwrap();
        let status = conn.status();
        assert!((200..300).contains(&status));
        let mut buf = [0; 4096];
        conn.read(&mut buf).unwrap();
        unsafe { println!("{}", str::from_utf8_unchecked(&buf)); }
    }
}