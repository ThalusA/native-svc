pub mod error;

use crate::error::HyperError;
use embedded_svc::http::client::Connection;
use embedded_svc::http::{Headers, Method, Status};
use embedded_svc::io::{ErrorType, Read, Write};
use http_body_util::Full;
use hyper::body::{Body, Bytes, Incoming};
use hyper::client::conn::http1::{SendRequest, handshake};
use hyper::header::{HeaderName, HeaderValue};
use hyper::{HeaderMap, Request, Response, Uri};
use hyper_util::rt::TokioIo;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;

pub struct HyperHttpConnection {
    rt: Runtime,
    sender: Option<SendRequest<Full<Bytes>>>,
    request: Option<Request<Full<Bytes>>>,
    response: Option<Response<Incoming>>,
    pending_read: Vec<u8>,
}

impl HyperHttpConnection {
    pub fn new() -> Self {
        Self {
            rt: Runtime::new().expect("failed to start tokio runtime"),
            sender: None,
            request: None,
            response: None,
            pending_read: Default::default(),
        }
    }

    pub fn connect(&mut self, url: Uri) {
        let host = url.host().expect("uri has no host");
        let port = url.port_u16().unwrap_or(80);
        let address = format!("{}:{}", host, port);
        let stream = self.rt.block_on(TcpStream::connect(address)).unwrap();
        let stream = TokioIo::new(stream);
        let (sender, conn) = self.rt.block_on(handshake(stream)).unwrap();
        self.rt.spawn(async move {
            if let Err(err) = conn.await {
                println!("Connection failed: {:?}", err);
            }
        });

        self.sender = Some(sender);
    }

    pub fn sender_mut(&mut self) -> &mut SendRequest<Full<Bytes>> {
        self.sender
            .as_mut()
            .expect("should have a connection established")
    }

    pub fn mut_request(&mut self) -> &mut Request<Full<Bytes>> {
        self.request.as_mut().expect("should be a request")
    }

    pub fn mut_response(&mut self) -> &mut Response<Incoming> {
        self.response.as_mut().expect("should be a response")
    }

    pub fn sender(&self) -> &SendRequest<Full<Bytes>> {
        self.sender
            .as_ref()
            .expect("should have a connection established")
    }

    pub fn request(&self) -> &Request<Full<Bytes>> {
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
        self.response()
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
    }
}

impl Read for HyperHttpConnection {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        if !self.pending_read.is_empty() {
            let length = self.pending_read.len().min(buffer.len());
            buffer[..length].copy_from_slice(&self.pending_read[..length]);
            self.pending_read = self.pending_read[length..].to_vec();
            return Ok(length);
        }
        let response_body = Pin::new(self.mut_response().body_mut());
        let mut cx = Context::from_waker(&Waker::noop());
        let frame = response_body
            .poll_frame(&mut cx)
            .map_ok(|frame| frame.into_data());
        let body = match frame {
            Poll::Ready(Some(Ok(Ok(bytes)))) => bytes,
            _ => return Ok(0),
        };
        let length = body.len().min(buffer.len());
        buffer[..length].copy_from_slice(&body[..length]);
        self.pending_read = buffer[length..].to_vec();
        Ok(length)
    }
}

impl Write for HyperHttpConnection {
    fn write(&mut self, buf: &[u8]) -> Result<usize, HyperError> {
        let request_body = Pin::new(self.mut_request().body_mut());
        let mut cx = Context::from_waker(&Waker::noop());
        let frame = request_body
            .poll_frame(&mut cx)
            .map_ok(|frame| frame.into_data());
        let body = match frame {
            Poll::Ready(Some(Ok(Ok(bytes)))) => bytes,
            _ => return Ok(0),
        };
        let mut buffer = vec![];
        buffer.extend_from_slice(&body);
        buffer.extend_from_slice(buf);
        *self.mut_request().body_mut() = Full::from(buffer);
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

    fn initiate_request<'a>(
        &'a mut self,
        method: Method,
        uri: &'a str,
        headers: &'a [(&'a str, &'a str)],
    ) -> Result<(), Self::Error> {
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
        let request = request.body(Full::from(Bytes::new())).unwrap();
        self.request = Some(request);
        self.response = None;
        Ok(())
    }

    fn is_request_initiated(&self) -> bool {
        self.request.is_some()
    }

    fn initiate_response(&mut self) -> Result<(), Self::Error> {
        let request = { self.request.take() };
        if let Some(request) = request {
            self.connect(request.uri().clone());
            if let Some(sender) = &mut self.sender {
                self.response = Some(self.rt.block_on(sender.send_request(request))?);
            } else {
                panic!("no sender");
            }
        } else {
            panic!("no request");
        }
        Ok(())
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
    use embedded_svc::http::client::Client;

    #[test]
    fn test_request_and_response_flow() {
        let conn = HyperHttpConnection::new();
        let mut client = Client::wrap(conn);

        let request = client.get("http://httpbin.org/get").unwrap();
        let mut response = request.submit().unwrap();

        let mut body = Vec::new();
        let mut buf = [0u8; 1024];

        loop {
            match response.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => body.extend_from_slice(&buf[..n]),
                Err(e) => panic!("{:?}", e),
            }
        }

        println!("{}", str::from_utf8(&body).unwrap());
    }

    #[test]
    fn test_write_body_and_send() {
        let conn = HyperHttpConnection::new();
        let mut client = Client::wrap(conn);

        let headers = &[("User-Agent", "TestAgent")];
        let request = client.post("http://httpbin.org/post", headers).unwrap();
        let mut response = request.submit().unwrap();

        let mut body = Vec::new();
        let mut buf = [0u8; 1024];

        loop {
            match response.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => body.extend_from_slice(&buf[..n]),
                Err(e) => panic!("{:?}", e),
            }
        }

        println!("{}", str::from_utf8(&body).unwrap());
    }
}
