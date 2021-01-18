//! This module crates a low-level client for kraken API handling required headers
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
use std::{convert::TryFrom, str::FromStr, time::SystemTime};
use url::{ParseError as UrlParseError, Url};

/// Configuration needed to initialize a Kraken client.
/// The key and secret aren't needed if only public APIs are used
#[derive(Default, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KrakenClientConfig {
    /// The name of the API key
    pub key: String,
    /// The API key secret
    pub secret: String,
}

/// A trait for objects representing a json schema that support a nonce field.
/// This is required by json scehmas that are arguments to private kraken APIs.
pub trait HasNonce: Serialize {
    /// Get nonce value
    fn get_nonce(&self) -> u64;
    /// Set nonce value
    fn set_nonce(&mut self, val: u64);
}

/// A low-level https connection to kraken that can execute public or private methods.
pub struct KrakenClient {
    client: reqwest::blocking::Client,
    config: KrakenClientConfig,
    base_url: Url,
    version: u16,
}

impl TryFrom<KrakenClientConfig> for KrakenClient {
    type Error = Error;
    fn try_from(config: KrakenClientConfig) -> Result<Self> {
        let base_url = Url::from_str("https://api.kraken.com/")?;
        let version = 0;
        let client = reqwest::blocking::ClientBuilder::new()
            .user_agent("krakenrs/0.0")
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
    fn query<D: Serialize, R: DeserializeOwned>(
        &mut self,
        url_path: &str,
        headers: HeaderMap,
        json_data: D,
    ) -> Result<R> {
        let url = self.base_url.join(url_path)?;

        let response = self
            .client
            .post(url)
            .headers(headers)
            .json(&json_data)
            .send()?;
        if !(response.status() == 200 || response.status() == 201 || response.status() == 202) {
            return Err(Error::BadStatus(response));
        }

        let result: R = response.json().map_err(Error::Json)?;
        Ok(result)
    }

    /// Execute a public API, given method, and object matching the expected schema, and returning expected schema or an error.
    pub fn query_public<D: Serialize, R: DeserializeOwned>(
        &mut self,
        method: &str,
        json_data: D,
    ) -> Result<R> {
        let url_path = format!("/{}/public/{}", self.version, method);

        self.query(&url_path, HeaderMap::new(), json_data)
    }

    /// Execute a private API, given method, and object matching the expected schema, and returning expected schema or an error.
    pub fn query_private<D: Serialize + HasNonce, R: DeserializeOwned>(
        &mut self,
        method: &str,
        mut json_data: D,
    ) -> Result<R> {
        if self.config.key.is_empty() || self.config.secret.is_empty() {
            return Err(Error::MissingCredentials);
        }

        let url_path = format!("/{}/private/{}", self.version, method);

        json_data.set_nonce(self.nonce()?);

        let mut headers = HeaderMap::new();
        headers.insert("API-Key", HeaderValue::from_str(&self.config.key)?);
        headers.insert(
            "API-Sign",
            HeaderValue::from_str(&self.sign(&json_data, &url_path)?)?,
        );

        self.query(&url_path, headers, json_data)
    }

    /// Get a nonce as suggsted by Kraken
    fn nonce(&self) -> Result<u64> {
        Ok(SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| Error::TimeError)?
            .as_millis() as u64)
    }

    /// Construct the payload signature using Kraken's scheme
    fn sign<D: HasNonce + Serialize>(&self, json_data: &D, url_path: &str) -> Result<String> {
        let nonce = json_data.get_nonce();
        let json_bytes = serde_json::to_string(json_data).map_err(Error::SigningJson)?;

        let sha2_result = {
            let mut hasher = Sha256::default();
            hasher.update(nonce.to_string());
            hasher.update(json_bytes);
            hasher.finalize()
        };

        let hmac_sha_key = base64::decode(&self.config.key).map_err(Error::SigningB64)?;

        type HmacSha = Hmac<Sha512>;
        let mut mac =
            HmacSha::new_varkey(&hmac_sha_key).expect("Hmac should work with any key length");
        mac.update(url_path.as_bytes());
        mac.update(&sha2_result);
        let mac = mac.finalize().into_bytes();
        Ok(base64::encode(&mac))
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
    /// json deserialization failed: {0}
    Json(reqwest::Error),
    /// Missing credentials required for private APIs
    MissingCredentials,
    /// Time error (preventing nonce computation)
    TimeError,
    /// json error during signing: {0}
    SigningJson(serde_json::Error),
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
