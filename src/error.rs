//! Error types for the krakenrs crate.

use displaydoc::Display;
use reqwest::header::InvalidHeaderValue;
use url::ParseError as UrlParseError;

/// Alias for Result that contains the error type for this crate
pub type Result<T> = core::result::Result<T, Error>;

/// An error that can be generated from the kraken client
#[derive(Display, Debug)]
pub enum Error {
    /// Failed forming URI: {0}
    Url(UrlParseError),
    /// Reqwest error: {0}
    Reqwest(reqwest::Error),
    /// kraken returned bad status: {0:?}
    BadStatus(reqwest::blocking::Response),
    /// kraken returned bad status code: {0}
    BadStatusCode(u16),
    /// json deserialization failed: {0}, body was: {1}
    Json(serde_json::Error, String),
    /// Kraken errors present: {0:?}
    KrakenErrors(Vec<String>),
    /// Missing result json
    MissingResultJson,
    /// Missing credentials required for private APIs
    MissingCredentials,
    /// Time error (preventing nonce computation)
    TimeError,
    /// Error serializing query string: {0}
    SerializingQs(serde_qs::Error),
    /// base64 error during signing: {0}
    SigningB64(base64ct::Error),
    /// Invalid header value: {0}
    InvalidHeader(InvalidHeaderValue),
}

impl From<UrlParseError> for Error {
    fn from(src: UrlParseError) -> Self {
        Self::Url(src)
    }
}

impl From<reqwest::Error> for Error {
    fn from(src: reqwest::Error) -> Self {
        Self::Reqwest(src)
    }
}

impl From<InvalidHeaderValue> for Error {
    fn from(src: InvalidHeaderValue) -> Self {
        Self::InvalidHeader(src)
    }
}

impl From<serde_qs::Error> for Error {
    fn from(src: serde_qs::Error) -> Self {
        Self::SerializingQs(src)
    }
}

impl std::error::Error for Error {}
