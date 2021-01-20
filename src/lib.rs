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
    /// Get the kraken system's time
    pub fn time(&mut self) -> Result<KrakenResult<Time>> {
        self.client.query_public("Time", Empty {})
    }
    /// Get the kraken system's status
    pub fn system_status(&mut self) -> Result<KrakenResult<SystemStatus>> {
        self.client.query_public("SystemStatus", Empty {})
    }
    /// Get the list of kraken's supported assets
    pub fn assets(&mut self) -> Result<KrakenResult<HashMap<String, AssetInfo>>> {
        self.client.query_public("Assets", Empty {})
    }
    /// Get the list of open orders
    pub fn get_open_orders(
        &mut self,
        userref: Option<UserRefId>,
    ) -> Result<KrakenResult<HashMap<TxId, OrderInfo>>> {
        self.client
            .query_private("GetOpenOrders", GetOpenOrdersRequest { nonce: 0, userref })
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
