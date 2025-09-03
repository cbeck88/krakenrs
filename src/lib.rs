//! A rust crate for making requests to the Kraken Rest API and subscribing
//! to Kraken websockets feeds

#![deny(missing_docs)]

mod last_and_data;
pub use last_and_data::LastAndData;

mod kraken_rest_client;
pub use kraken_rest_client::*;

mod messages;
use messages::{
    AddOrderRequest, AssetPairsRequest, CancelAllOrdersAfterRequest, CancelOrderRequest, Empty, GetOHLCDataRequest,
    GetOpenOrdersRequest, GetRecentTradesRequest, GetTradeVolumeRequest, KrakenResult, TickerRequest,
    unpack_kraken_result,
};
pub use messages::{
    AddOrderResponse, AssetInfo, AssetPair, AssetPairsResponse, AssetTickerInfo, AssetsResponse, BalanceResponse,
    BsType, CancelAllOrdersAfterResponse, CancelAllOrdersResponse, CancelOrderResponse, FeeTierInfo,
    GetOHLCDataResponse, GetOpenOrdersResponse, GetRecentTradesResponse, GetTradeVolumeResponse,
    GetWebSocketsTokenResponse, OrderAdded, OrderFlag, OrderInfo, OrderStatus, OrderType, SystemStatusResponse,
    TickerResponse, TimeResponse, TxId, UserRefId,
};

use core::convert::TryFrom;
use std::collections::BTreeSet;

// Websockets API support
#[cfg(feature = "ws")]
pub mod ws;

/// A description of a market order to place
#[derive(Debug, Clone)]
pub struct MarketOrder {
    /// Whether to buy or sell
    pub bs_type: BsType,
    /// Volume (in lots)
    pub volume: String,
    /// Asset pair
    pub pair: String,
    /// Order flags (market price protection etc.)
    pub oflags: BTreeSet<OrderFlag>,
}

/// A description of a limit order to place
#[derive(Debug, Clone)]
pub struct LimitOrder {
    /// Whether to buy or sell
    pub bs_type: BsType,
    /// Volume (in lots)
    pub volume: String,
    /// Asset pair
    pub pair: String,
    /// Price
    pub price: String,
    /// Order flags (post-only etc.)
    pub oflags: BTreeSet<OrderFlag>,
}

/// A connection to the Kraken REST API
/// This only supports blocking http requests for now
pub struct KrakenRestAPI {
    client: KrakenRestClient,
}

impl KrakenRestAPI {
    /// Try to create RestAPI instance from config
    pub fn new(src: KrakenRestConfig) -> Result<Self> {
        Ok(KrakenRestAPI {
            client: KrakenRestClient::try_from(src)?,
        })
    }

    /// (Public) Get the kraken system's time
    pub fn time(&self) -> Result<TimeResponse> {
        let result: Result<KrakenResult<TimeResponse>> = self.client.query_public("Time", Empty {});
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get the kraken system's status
    pub fn system_status(&self) -> Result<SystemStatusResponse> {
        let result: Result<KrakenResult<SystemStatusResponse>> = self.client.query_public("SystemStatus", Empty {});
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get the list of kraken's supported assets, and info
    pub fn assets(&self) -> Result<AssetsResponse> {
        let result: Result<KrakenResult<AssetsResponse>> = self.client.query_public("Assets", Empty {});
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get the list of kraken's asset pairs, and info
    ///
    /// Arguments:
    /// * pairs: A list of Kraken asset pair strings to get info about. If empty then all asset pairs
    pub fn asset_pairs(&self, pairs: Vec<String>) -> Result<AssetPairsResponse> {
        let result: Result<KrakenResult<AssetPairsResponse>> = self
            .client
            .query_public("AssetPairs", AssetPairsRequest { pair: pairs.join(",") });
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get the ticker price for one or more asset pairs
    ///
    /// Arguments:
    /// * pairs: A list of Kraken asset pair strings to get ticker info about
    pub fn ticker(&self, pairs: Vec<String>) -> Result<TickerResponse> {
        let result: Result<KrakenResult<TickerResponse>> = self
            .client
            .query_public("Ticker", TickerRequest { pair: pairs.join(",") });
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get OHLC data for an asset pair, at one minute intervals.
    /// Optionally pass "since", and will only return data after that timestamp.
    /// (Intended for incremental updates).
    /// Returns up to 720 of the most recent entries.
    /// Older data cannot be retrieved, regardless of the value of since.
    ///
    /// Arguments:
    /// * pair: Which asset pair to get data for
    /// * since: A timestamp to get data since
    pub fn ohlc(&self, pair: String, since: Option<String>) -> Result<GetOHLCDataResponse> {
        let result: Result<KrakenResult<GetOHLCDataResponse>> = self.client.query_public(
            "OHLC",
            GetOHLCDataRequest {
                pair,
                since,
                interval: None,
            },
        );
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get OHLC data for an asset pair, at user-specified interval number of minutes.
    /// Valid intervals are 1, 5, 15, 30, 60, 240, 1440, 10080, 21600.
    ///
    /// Optionally pass "since", and will only return data after that timestamp.
    /// (Intended for incremental updates).
    /// Returns up to 720 of the most recent entries.
    /// Older data cannot be retrieved, regardless of the value of since.
    ///
    /// Arguments:
    /// * pair: Which asset pair to get data for
    /// * since: A timestamp to get data since
    pub fn ohlc_at_interval(&self, pair: String, interval: u16, since: Option<String>) -> Result<GetOHLCDataResponse> {
        let result: Result<KrakenResult<GetOHLCDataResponse>> = self.client.query_public(
            "OHLC",
            GetOHLCDataRequest {
                pair,
                since,
                interval: Some(interval),
            },
        );
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get 1000 most recent trades in an asset pair, optionally, since a particular timestamp.
    /// The response contains a "last" number which can be used as "since" to get the next page if desired.
    pub fn get_recent_trades(&self, pair: String, since: Option<String>) -> Result<GetRecentTradesResponse> {
        let result: Result<KrakenResult<GetRecentTradesResponse>> = self.client.query_public(
            "Trades",
            GetRecentTradesRequest {
                pair,
                since,
                count: None,
            },
        );
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get the balance
    pub fn get_account_balance(&self) -> Result<BalanceResponse> {
        let result: Result<KrakenResult<BalanceResponse>> = self.client.query_private("Balance", Empty {});
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get trade volume and fee tier info, per asset pair
    pub fn get_trade_volume(&self, asset_pairs: Vec<String>) -> Result<GetTradeVolumeResponse> {
        let result: Result<KrakenResult<GetTradeVolumeResponse>> = self.client.query_private(
            "GetTradeVolume",
            GetTradeVolumeRequest {
                pair: asset_pairs.join(","),
            },
        );
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get a websockets authentication token
    pub fn get_websockets_token(&self) -> Result<GetWebSocketsTokenResponse> {
        let result: Result<KrakenResult<GetWebSocketsTokenResponse>> =
            self.client.query_private("GetWebSocketsToken", Empty {});
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get the list of open orders
    ///
    /// Arguments:
    /// * userref: An optional user-reference to filter the list of open orders by
    pub fn get_open_orders(&self, userref: Option<UserRefId>) -> Result<GetOpenOrdersResponse> {
        let result: Result<KrakenResult<GetOpenOrdersResponse>> = self
            .client
            .query_private("OpenOrders", GetOpenOrdersRequest { userref });
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Cancel order
    ///
    /// Arguments:
    /// * id: A TxId (OR a UserRefId) of order(s) to cancel
    pub fn cancel_order(&self, id: String) -> Result<CancelOrderResponse> {
        let result: Result<KrakenResult<CancelOrderResponse>> = self
            .client
            .query_private("CancelOrder", CancelOrderRequest { txid: id });
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Cancel all orders (regardless of user ref or tx id)
    pub fn cancel_all_orders(&self) -> Result<CancelAllOrdersResponse> {
        let result: Result<KrakenResult<CancelAllOrdersResponse>> = self.client.query_private("CancelAll", Empty {});
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Cancel all orders after
    ///
    /// Arguments:
    /// * timeout: Integer timeout specified in seconds. 0 to disable the timer.
    pub fn cancel_all_orders_after(&self, timeout: u64) -> Result<CancelAllOrdersAfterResponse> {
        let result: Result<KrakenResult<CancelAllOrdersAfterResponse>> = self
            .client
            .query_private("CancelAllOrdersAfter", CancelAllOrdersAfterRequest { timeout });
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Place a market order
    ///
    /// Arguments:
    /// * market_order: Market order object describing the parameters of the order
    /// * user_ref_id: Optional user ref id to attach to the order
    /// * validate: If true, the order is only validated and is not actually placed
    pub fn add_market_order(
        &self,
        market_order: MarketOrder,
        user_ref_id: Option<UserRefId>,
        validate: bool,
    ) -> Result<AddOrderResponse> {
        let req = AddOrderRequest {
            ordertype: OrderType::Market,
            bs_type: market_order.bs_type,
            volume: market_order.volume,
            pair: market_order.pair,
            price: Default::default(),
            oflags: market_order.oflags,
            userref: user_ref_id,
            validate,
        };
        let result: Result<KrakenResult<AddOrderResponse>> = self.client.query_private("AddOrder", req);
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Place a limit order
    ///
    /// Arguments:
    /// * limit_order: Limit order object describing the parameters of the order
    /// * user_ref_id: Optional user ref id to attach to the order
    /// * validate: If true, the order is only validated and is not actually placed
    pub fn add_limit_order(
        &self,
        limit_order: LimitOrder,
        user_ref_id: Option<UserRefId>,
        validate: bool,
    ) -> Result<AddOrderResponse> {
        let req = AddOrderRequest {
            ordertype: OrderType::Limit,
            bs_type: limit_order.bs_type,
            volume: limit_order.volume,
            pair: limit_order.pair,
            price: limit_order.price,
            oflags: limit_order.oflags,
            userref: user_ref_id,
            validate,
        };
        let result: Result<KrakenResult<AddOrderResponse>> = self.client.query_private("AddOrder", req);
        result.and_then(unpack_kraken_result)
    }
}

impl TryFrom<KrakenRestConfig> for KrakenRestAPI {
    type Error = Error;
    fn try_from(src: KrakenRestConfig) -> Result<Self> {
        Self::new(src)
    }
}
