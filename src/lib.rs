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
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use tokio::runtime::Runtime;

/// Default buffer size for writing into the request body.
const DEFAULT_BUFFER_SIZE: usize = 8192;

type HyperClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>;

pub struct HyperHttpConnection {
    rt: Runtime,
    client: HyperClient,
    request: Option<Request<Full<Bytes>>>,
    response: Option<Response<Incoming>>,
    read_buffer: Bytes,
    write_buffer: Vec<u8>,
}

impl HyperHttpConnection {
    pub fn new() -> Result<Self, HyperError> {
        let https = HttpsConnector::new();
        let client = Client::builder(TokioExecutor::new()).build(https);
        let rt = Runtime::new().map_err(HyperError::RuntimeCreation)?;

        Ok(Self {
            rt,
            client,
            request: None,
            response: None,
            read_buffer: Bytes::new(),
            write_buffer: Vec::with_capacity(DEFAULT_BUFFER_SIZE),
        })
    }

    /// Méthode helper pour mapper les méthodes HTTP
    fn map_method(method: Method) -> Result<hyper::Method, HyperError> {
        match method {
            Method::Delete => Ok(hyper::Method::DELETE),
            Method::Get => Ok(hyper::Method::GET),
            Method::Head => Ok(hyper::Method::HEAD),
            Method::Post => Ok(hyper::Method::POST),
            Method::Put => Ok(hyper::Method::PUT),
            Method::Connect => Ok(hyper::Method::CONNECT),
            Method::Options => Ok(hyper::Method::OPTIONS),
            Method::Trace => Ok(hyper::Method::TRACE),
            Method::Patch => Ok(hyper::Method::PATCH),
            _ => Err(HyperError::UnsupportedMethod(format!("{:?}", method))),
        }
    }

    /// Construit la HeaderMap à partir des headers fournis
    fn build_headers(headers: &[(&str, &str)]) -> Result<HeaderMap, HyperError> {
        let mut header_map = HeaderMap::with_capacity(headers.len());

        for &(name, value) in headers {
            let header_name =
                HeaderName::from_bytes(name.as_bytes()).map_err(HyperError::InvalidHeaderName)?;
            let header_value =
                HeaderValue::from_str(value).map_err(HyperError::InvalidHeaderValue)?;
            header_map.insert(header_name, header_value);
        }

        Ok(header_map)
    }

    /// Vérifie qu'une réponse existe
    fn ensure_response(&self) -> Result<&Response<Incoming>, HyperError> {
        self.response.as_ref().ok_or(HyperError::NoResponse)
    }

    /// Charge le body de la réponse dans le buffer de lecture
    fn load_response_body(&mut self) -> Result<(), HyperError> {
        if let Some(mut response) = self.response.take() {
            let body_future = response.body_mut().collect();
            let body = self.rt.block_on(body_future).map_err(HyperError::Hyper)?;
            self.read_buffer = body.to_bytes();
        }
        Ok(())
    }
}

impl Default for HyperHttpConnection {
    fn default() -> Self {
        Self::new().expect("Failed to create HyperHttpConnection")
    }
}

impl ErrorType for HyperHttpConnection {
    type Error = HyperError;
}

impl Status for HyperHttpConnection {
    fn status(&self) -> u16 {
        self.ensure_response()
            .map(|response| response.status().as_u16())
            .unwrap_or(500)
    }

    fn status_message(&self) -> Option<&'_ str> {
        self.ensure_response()
            .ok()
            .and_then(|response| response.status().canonical_reason())
    }
}

impl Headers for HyperHttpConnection {
    fn header(&self, name: &str) -> Option<&'_ str> {
        self.ensure_response()
            .ok()
            .and_then(|response| response.headers().get(name))
            .and_then(|value| value.to_str().ok())
    }
}

impl Read for HyperHttpConnection {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        // Charger le body si le buffer est vide et qu'on a une réponse
        if self.read_buffer.is_empty() && self.response.is_some() {
            self.load_response_body()?;
        }

        if self.read_buffer.is_empty() {
            return Ok(0); // EOF
        }

        let length = self.read_buffer.len().min(buffer.len());
        buffer[..length].copy_from_slice(&self.read_buffer[..length]);

        // Utiliser slice pour éviter la copie
        self.read_buffer = self.read_buffer.slice(length..);

        Ok(length)
    }
}

impl Write for HyperHttpConnection {
    fn write(&mut self, buf: &[u8]) -> Result<usize, HyperError> {
        self.write_buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), HyperError> {
        let request = self.request.as_mut().ok_or(HyperError::NoRequest)?;

        let body_data = std::mem::take(&mut self.write_buffer);
        *request.body_mut() = Full::from(body_data);

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
        let mapped_method = Self::map_method(method)?;
        let header_map = Self::build_headers(headers)?;

        let mut request_builder = Request::builder().method(mapped_method).uri(uri);

        // Gestion plus propre des headers
        if let Some(headers_mut) = request_builder.headers_mut() {
            headers_mut.extend(header_map);
        }

        let request = request_builder
            .body(Full::from(Bytes::new()))
            .map_err(HyperError::Http)?;

        self.request = Some(request);
        self.response = None;
        self.read_buffer = Bytes::new();
        self.write_buffer.clear();

        Ok(())
    }

    fn is_request_initiated(&self) -> bool {
        self.request.is_some()
    }

    fn initiate_response(&mut self) -> Result<(), Self::Error> {
        let request = self.request.take().ok_or(HyperError::NoRequest)?;

        let response_future = self.client.request(request);
        let response = self
            .rt
            .block_on(response_future)
            .map_err(HyperError::Client)?;

        self.response = Some(response);
        Ok(())
    }

    fn is_response_initiated(&self) -> bool {
        self.response.is_some()
    }

    fn split(&mut self) -> (&Self::Headers, &mut Self::Read) {
        // Utilisation d'un pointeur sûr
        let headers: *const Self = self;
        let headers = unsafe { &*headers };
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
        let conn = HyperHttpConnection::new().unwrap();
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
        let conn = HyperHttpConnection::new().unwrap();
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
