//! This module provides a low-level client for kraken API handling required headers
//! and serialization. It is similar to krakenex python code, but less messy.
//! https://github.com/veox/python3-krakenex/blob/master/krakenex/api.py

use displaydoc::Display;
use hmac::{Hmac, Mac, NewMac};
use reqwest::{
    blocking::Response,
    header::{HeaderMap, HeaderValue, InvalidHeaderValue},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha512};
use std::{
    convert::TryFrom,
    str::FromStr,
    time::{Duration, SystemTime},
};
use url::{ParseError as UrlParseError, Url};

/// Configuration needed to initialize a Kraken client.
/// The credentials aren't needed if only public APIs are used
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KrakenClientConfig {
    /// The timeout to use for http connections
    /// Recommended is to use 30s.
    pub timeout: Duration,
    /// The credentials (if using private APIs)
    pub creds: KrakenCredentials,
}

impl Default for KrakenClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::new(30, 0),
            creds: Default::default(),
        }
    }
}

/// Credentials needed to use private Kraken APIs.
#[derive(Default, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KrakenCredentials {
    /// The name of the API key
    pub key: String,
    /// The API key secret
    pub secret: String,
}

/// A low-level https connection to kraken that can execute public or private methods.
pub struct KrakenClient {
    /// Http client
    client: reqwest::blocking::Client,
    /// Our configuration
    config: KrakenClientConfig,
    /// Base url to contact kraken at
    base_url: Url,
    /// Kraken Api version to connect to
    version: u16,
}

// KrakenRS version
const KRAKEN_RS_VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

impl TryFrom<KrakenClientConfig> for KrakenClient {
    type Error = Error;
    fn try_from(config: KrakenClientConfig) -> Result<Self> {
        let base_url = Url::from_str("https://api.kraken.com/")?;
        let version = 0;
        let client = reqwest::blocking::ClientBuilder::new()
            .user_agent(format!(
                "krakenrs/{}",
                KRAKEN_RS_VERSION.unwrap_or("unknown")
            ))
            .timeout(Some(config.timeout))
            .build()?;
        Ok(Self {
            base_url,
            version,
            client,
            config,
        })
    }
}

impl KrakenClient {
    /// Change the credentials used
    pub fn set_creds(&mut self, creds: KrakenCredentials) {
        self.config.creds = creds;
    }

    /// Execute a public API, given method, and object matching the expected schema, and returning expected schema or an error.
    pub fn query_public<D: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        query_data: D,
    ) -> Result<R> {
        let url_path = format!("/{}/public/{}", self.version, method);

        let post_data = serde_qs::to_string(&query_data)?;
        //eprintln!("post_data = {}", post_data);

        self.query(&url_path, HeaderMap::new(), post_data)
    }

    /// Execute a private API, given method, and object matching the expected schema, and returning expected schema or an error.
    pub fn query_private<D: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        query_data: D,
    ) -> Result<R> {
        if self.config.creds.key.is_empty() || self.config.creds.secret.is_empty() {
            return Err(Error::MissingCredentials);
        }

        let url_path = format!("/{}/private/{}", self.version, method);

        // Sign the query data and url path, resulting in encoded post_data with nonce, and a signature.
        let (post_data, sig) = self.sign(query_data, &url_path)?;

        let mut headers = HeaderMap::new();
        headers.insert("API-Key", HeaderValue::from_str(&self.config.creds.key)?);
        headers.insert("API-Sign", HeaderValue::from_str(&sig)?);

        self.query(&url_path, headers, post_data)
    }

    /// Send a query (public or private) to kraken API, and interpret response as JSON
    fn query<R: DeserializeOwned>(
        &self,
        url_path: &str,
        headers: HeaderMap,
        post_data: String,
    ) -> Result<R> {
        let url = self.base_url.join(url_path)?;

        //eprintln!("POST {}\n{}", url_path, post_data);

        let response = self
            .client
            .post(url)
            .headers(headers)
            .body(post_data)
            .send()?;
        if !(response.status() == 200 || response.status() == 201 || response.status() == 202) {
            return Err(Error::BadStatus(response));
        }

        let text = response.text()?;

        let result: R =
            serde_json::from_str(&text).map_err(|err| Error::Json(err, text.clone()))?;
        Ok(result)
    }

    /// Serialize a json payload, adding a nonce, and producing a signature using Kraken's scheme
    ///
    /// Arguments:
    /// * query_data for the request, with "nonce" value not yet assigned
    /// * url path for the request
    ///
    /// Returns:
    /// * post_data for the request (encoded query data, with nonce added)
    /// * signature over that post data string
    fn sign<D: Serialize>(&self, query_data: D, url_path: &str) -> Result<(String, String)> {
        // Generate a nonce to become part of the postdata
        let nonce = Self::nonce()?;
        // Convert the data to a query string
        let qs = serde_qs::to_string(&query_data)?;
        // Append nonce to query string
        let post_data = if qs.is_empty() {
            format!("nonce={}", nonce)
        } else {
            format!("nonce={}&{}", nonce, qs)
        };

        let sha2_result = {
            let mut hasher = Sha256::default();
            hasher.update(nonce.to_string());
            hasher.update(&post_data);
            hasher.finalize()
        };

        let hmac_sha_key = base64::decode(&self.config.creds.secret).map_err(Error::SigningB64)?;

        type HmacSha = Hmac<Sha512>;
        let mut mac =
            HmacSha::new_varkey(&hmac_sha_key).expect("Hmac should work with any key length");
        mac.update(url_path.as_bytes());
        mac.update(&sha2_result);
        let mac = mac.finalize().into_bytes();

        let sig = base64::encode(&mac);
        Ok((post_data, sig))
    }

    /// Get a nonce as suggsted by Kraken
    fn nonce() -> Result<u64> {
        Ok(SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| Error::TimeError)?
            .as_millis() as u64)
    }
}

/// Alias for Result that contains the error type for this crate
pub type Result<T> = core::result::Result<T, Error>;

/// An error that can be generated from the low-level kraken client
#[derive(Display, Debug)]
pub enum Error {
    /// Failed forming URI: {0}
    Url(UrlParseError),
    /// Reqwest error: {0}
    Reqwest(reqwest::Error),
    /// kraken returned bad status: {0:?}
    BadStatus(Response),
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
    SigningB64(base64::DecodeError),
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
