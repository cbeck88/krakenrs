//! A rust crate for making requests to the Kraken API

#![deny(missing_docs)]

mod kraken_client;
pub use kraken_client::*;

mod messages;
pub use messages::*;

use core::convert::TryFrom;

/// A connection to the Kraken API
/// This only supports blocking http requests for now
pub struct KrakenAPI {
    client: KrakenClient,
}

impl KrakenAPI {
    /// Get the kraken system's time
    pub fn time(&mut self) -> Result<KrakenResult<Time>> {
        self.client.query_public("Time", Empty {})
    }
    /// Get the kraken system's status
    pub fn system_status(&mut self) -> Result<KrakenResult<SystemStatus>> {
        self.client.query_public("SystemStatus", Empty {})
    }
}

impl TryFrom<KrakenClientConfig> for KrakenAPI {
    type Error = Error;
    fn try_from(src: KrakenClientConfig) -> Result<Self> {
        Ok(KrakenAPI {
            client: KrakenClient::try_from(src)?,
        })
    }
}
