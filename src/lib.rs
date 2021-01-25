//! A rust crate for making requests to the Kraken API

#![deny(missing_docs)]

mod kraken_client;
pub use kraken_client::*;

mod messages;
pub use messages::*;

use core::convert::TryFrom;
use std::collections::HashMap;

/// A connection to the Kraken API
/// This only supports blocking http requests for now
pub struct KrakenAPI {
    client: KrakenClient,
}

impl KrakenAPI {
    /// (Public) Get the kraken system's time
    pub fn time(&mut self) -> Result<TimeResponse> {
        let result: Result<KrakenResult<TimeResponse>> = self.client.query_public("Time", Empty {});
        result.and_then(unpack_kraken_result)
    }
    /// (Public) Get the kraken system's status
    pub fn system_status(&mut self) -> Result<SystemStatusResponse> {
        let result: Result<KrakenResult<SystemStatusResponse>> =
            self.client.query_public("SystemStatus", Empty {});
        result.and_then(unpack_kraken_result)
    }
    /// (Public) Get the list of kraken's supported assets
    pub fn assets(&mut self) -> Result<HashMap<String, AssetInfo>> {
        let result: Result<KrakenResult<HashMap<String, AssetInfo>>> =
            self.client.query_public("Assets", Empty {});
        result.and_then(unpack_kraken_result)
    }
    /// (Private) Get the list of open orders
    pub fn get_open_orders(&mut self, userref: Option<UserRefId>) -> Result<GetOpenOrdersResponse> {
        let result: Result<KrakenResult<GetOpenOrdersResponse>> = self
            .client
            .query_private("OpenOrders", GetOpenOrdersRequest { userref });
        result.and_then(unpack_kraken_result)
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
