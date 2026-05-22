//! Minimal synchronous HTTP abstraction used by the CLI.
//!
//! The wasm plugin calls Zellij's async `web_request` directly and does
//! not go through this trait — but they share the request/response
//! shapes so we can pass parsed bodies through the same deserializers.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpVerb {
    Get,
    Post,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn body_as_str(&self) -> &str {
        std::str::from_utf8(&self.body).unwrap_or("")
    }
}

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("non-success status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("deserialization error: {0}")]
    Decode(String),
}

pub trait HttpClient {
    fn request(
        &self,
        url: &str,
        verb: HttpVerb,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> Result<HttpResponse, HttpError>;
}
