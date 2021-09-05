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
    pub fn assets(&mut self) -> Result<AssetsResponse> {
        let result: Result<KrakenResult<AssetsResponse>> =
            self.client.query_public("Assets", Empty {});
        result.and_then(unpack_kraken_result)
    }
    /// (Public) Get the list of kraken's supported assets
    ///
    /// Arguments:
    /// * pairs: A list of Kraken asset pair strings to get info about
    pub fn asset_pairs(&mut self, pairs: Vec<String>) -> Result<AssetPairsResponse> {
        let result: Result<KrakenResult<AssetPairsResponse>> = if pairs.is_empty() {
            self.client.query_public("AssetPairs", Empty {})
        } else {
            self.client.query_public(
                "AssetPairs",
                AssetPairsRequest {
                    pair: pairs.join(","),
                },
            )
        };
        result.and_then(unpack_kraken_result)
    }
    /// (Private) Get the balance
    pub fn get_account_balance(&mut self) -> Result<BalanceResponse> {
        let result: Result<KrakenResult<BalanceResponse>> =
            self.client.query_private("Balance", Empty {});
        result.and_then(unpack_kraken_result)
    }
    /// (Private) Get the list of open orders
    ///
    /// Arguments:
    /// * userref: An optional user-reference to filter the list of open orders by
    pub fn get_open_orders(&mut self, userref: Option<UserRefId>) -> Result<GetOpenOrdersResponse> {
        let result: Result<KrakenResult<GetOpenOrdersResponse>> = self
            .client
            .query_private("OpenOrders", GetOpenOrdersRequest { userref });
        result.and_then(unpack_kraken_result)
    }
    /// (Private) Cancel order
    ///
    /// Arguments:
    /// * id: A TxId (OR a UserRefId) of order(s) to cancel
    pub fn cancel_order(&mut self, id: String) -> Result<CancelOrderResponse> {
        let result: Result<KrakenResult<CancelOrderResponse>> = self
            .client
            .query_private("CancelOrder", CancelOrderRequest { txid: id });
        result.and_then(unpack_kraken_result)
    }
    /// (Private) Cancel all orders (regardless of user ref or tx id)
    pub fn cancel_all_orders(&mut self) -> Result<CancelAllOrdersResponse> {
        let result: Result<KrakenResult<CancelAllOrdersResponse>> =
            self.client.query_private("CancelAll", Empty {});
        result.and_then(unpack_kraken_result)
    }
    /// (Private) Cancel all orders after
    ///
    /// Arguments:
    /// * timeout: Integer timeout specified in seconds. 0 to disable the timer.
    pub fn cancel_all_orders_after(
        &mut self,
        timeout: u64,
    ) -> Result<CancelAllOrdersAfterResponse> {
        let result: Result<KrakenResult<CancelAllOrdersAfterResponse>> = self.client.query_private(
            "CancelAllOrdersAfter",
            CancelAllOrdersAfterRequest { timeout },
        );
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
