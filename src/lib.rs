pub mod error;

use crate::error::HyperError;
use embedded_svc::http::client::Connection;
use embedded_svc::http::{Headers, Method, Status};
use embedded_svc::io::{ErrorType, Read, Write};
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HeaderName, HeaderValue};
use hyper::{HeaderMap, Request, Response};
use hyper_tls::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use tokio::runtime::Runtime;

pub struct HyperHttpConnection {
    rt: Runtime,
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    request: Option<Request<Full<Bytes>>>,
    response: Option<Response<Incoming>>,
    pending_read: Vec<u8>,
    pending_write: Vec<u8>,
}

impl HyperHttpConnection {
    pub fn new() -> Self {
        let https = HttpsConnector::new();
        let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build(https);
        Self {
            rt: Runtime::new().expect("failed to start tokio runtime"),
            client,
            request: None,
            response: None,
            pending_read: vec![],
            pending_write: vec![],
        }
    }
    
    
}

impl ErrorType for HyperHttpConnection {
    type Error = HyperError;
}

impl Status for HyperHttpConnection {
    fn status(&self) -> u16 {
        self.response
            .as_ref()
            .expect("should be a response")
            .status()
            .as_u16()
    }

    fn status_message(&self) -> Option<&'_ str> {
        self.response
            .as_ref()
            .expect("should be a response")
            .status()
            .canonical_reason()
    }
}

impl Headers for HyperHttpConnection {
    fn header(&self, name: &str) -> Option<&'_ str> {
        self.response
            .as_ref()
            .expect("should be a response")
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
    }
}

impl Read for HyperHttpConnection {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        if self.pending_read.is_empty() {
            match &mut self.response {
                Some(response) => {
                    let body = response.body_mut().collect();
                    let bytes = self.rt.block_on(body)?;
                    let bytes = bytes.to_bytes().to_vec();
                    self.pending_read = bytes;
                    self.response = None;
                }
                None => return Ok(0),
            }
        }
        let length = self.pending_read.len().min(buffer.len());
        buffer[..length].copy_from_slice(&self.pending_read[..length]);
        self.pending_read.drain(..length);
        Ok(length)
    }
}

impl Write for HyperHttpConnection {
    fn write(&mut self, buf: &[u8]) -> Result<usize, HyperError> {
        self.pending_write.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), HyperError> {
        *self
            .request
            .as_mut()
            .expect("should be a request")
            .body_mut() = Full::from(self.pending_write.clone());
        self.pending_write.clear();
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
            self.response = self.rt.block_on(self.client.request(request)).ok();
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

        let request = client.get("https://httpbin.org/get").unwrap();
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

        let body = r#"{"test": 2}"#;
        let len = body.len().to_string();
        let headers = &[
            ("User-Agent", "TestAgent"),
            ("Content-Type", "application/json"),
            ("Content-Length", &len),
        ];
        let mut request = client.post("https://httpbin.org/post", headers).unwrap();
        request.write(body.as_bytes()).unwrap();
        request.flush().unwrap();
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
