# native-svc

[![Rust](https://img.shields.io/badge/rust-1.88.0%2B-orange.svg)](https://www.rust-lang.org)
[![Crates.io](https://img.shields.io/crates/v/native-svc)](https://crates.io/crates/native-svc)
[![Documentation](https://docs.rs/native-svc/badge.svg)](https://docs.rs/native-svc)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

An HTTP adapter that implements the `embedded-svc` interface using `hyper` as the backend.

This library enables HTTP/HTTPS requests with a familiar synchronous API while leveraging the power and robustness of `hyper` under the hood.

## 🚀 Features

- **Synchronous Interface**: Simple and familiar API based on `embedded-svc`
- **HTTPS Support**: Secure TLS connections via `hyper-tls`
- **Body Handling**: Complete support for reading and writing request/response bodies
- **HTTP Headers**: Full header management with validation
- **Error Handling**: Detailed and ergonomic error types
- **Performance**: Built on `hyper` and `tokio` for optimal performance

## 📦 Installation

Add this to your `Cargo.toml`:
```toml
[dependencies]
native-svc = "0.1.0"
```
## 🛠️ Usage

### Simple GET Request
```rust
use native_svc::HyperHttpConnection;
use embedded_svc::http::client::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
// Create a connection
let conn = HyperHttpConnection::new()?;
let mut client = Client::wrap(conn);

    // Perform a GET request
    let request = client.get("https://httpbin.org/get")?;
    let mut response = request.submit()?;

    // Read the response
    let mut body = Vec::new();
    let mut buf = [0u8; 1024];
    
    while let Ok(n) = response.read(&mut buf) {
        if n == 0 { break; }
        body.extend_from_slice(&buf[..n]);
    }

    println!("Status: {}", response.status());
    println!("Body: {}", String::from_utf8_lossy(&body));
    
    Ok(())
}
```
### POST Request with JSON
```rust
use native_svc::HyperHttpConnection;
use embedded_svc::http::client::Client;
use embedded_svc::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
let conn = HyperHttpConnection::new()?;
let mut client = Client::wrap(conn);

    // Prepare JSON data
    let json_data = r#"{"name": "John", "age": 30}"#;
    let headers = &[
        ("Content-Type", "application/json"),
        ("Content-Length", &json_data.len().to_string()),
    ];

    // Create and send the request
    let mut request = client.post("https://httpbin.org/post", headers)?;
    request.write_all(json_data.as_bytes())?;
    request.flush()?;
    
    let mut response = request.submit()?;

    // Process the response
    println!("Status: {}", response.status());
    if let Some(content_type) = response.header("content-type") {
        println!("Content-Type: {}", content_type);
    }

    Ok(())
}
```
### Error Handling
```rust
use native_svc::{HyperHttpConnection, HyperError};

fn make_request() -> Result<String, HyperError> {
let conn = HyperHttpConnection::new()?;
let mut client = embedded_svc::http::client::Client::wrap(conn);

    let request = client.get("https://example.com")?;
    let mut response = request.submit()?;
    
    let mut body = String::new();
    let mut buffer = [0u8; 1024];
    
    loop {
        match response.read(&mut buffer)? {
            0 => break,
            n => body.push_str(&String::from_utf8_lossy(&buffer[..n])),
        }
    }
    
    Ok(body)
}
```
## 🏗️ Architecture

The library is organized into modules:

- **`lib.rs`**: Main `HyperHttpConnection` structure and trait implementations
- **`error.rs`**: Custom error types with detailed error handling

### Implemented Traits

- `embedded_svc::http::client::Connection`: Main interface for HTTP connections
- `embedded_svc::io::Read`: Reading response body
- `embedded_svc::io::Write`: Writing request body
- `embedded_svc::http::Status`: HTTP status access
- `embedded_svc::http::Headers`: HTTP headers access

## 🧪 Testing

Run tests with:
```bash
cargo test
```
**Note**: Integration tests require an Internet connection as they use `httpbin.org`.

## 📊 Performance

- **Runtime**: Uses Tokio with a multi-threaded runtime
- **Memory**: Default internal buffer of 8KB for write operations
- **Connections**: HTTPS connections support with connection reuse via `hyper`

## 🔒 Security

- Native TLS/SSL support via `hyper-tls`

## 🤝 Contributing

Contributions are welcome! Here's how to contribute:

1. Fork the project
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

### Development Guidelines

- Follow Rust naming conventions
- Add tests for new functionality
- Update documentation for public APIs
- Ensure `cargo clippy` passes without warnings

## 📄 License

This project is licensed under MIT. See the [LICENSE](LICENSE) files for details.

## 🔄 Compatibility

- **Rust Version**: 1.88.0 or later
- **Platforms**: All platforms supported by `tokio` and `hyper`
- **HTTP Versions**: HTTP/1.1

## 🙏 Acknowledgments

- The [hyper](https://github.com/hyperium/hyper) team for their excellent HTTP client
- The [embedded-svc](https://github.com/esp-rs/embedded-svc) team for standardized traits
- The Rust community for the incredible ecosystem

## 🐛 Known Issues

- Limited HTTP/2 support (HTTP/1.1 only currently)

---

**Made with ❤️ in Rust**
