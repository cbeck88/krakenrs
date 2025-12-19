//! Async (non-blocking) versions of the Kraken REST client and API.
//!
//! This module provides async versions of [KrakenRestClient] and [KrakenRestAPI]
//! that use async/await instead of blocking HTTP calls.
//!
//! # Example
//!
//! ```ignore
//! use krakenrs::non_blocking::KrakenRestAPI;
//! use krakenrs::KrakenRestConfig;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = KrakenRestConfig::default();
//!     let api = KrakenRestAPI::new(config).unwrap();
//!     let time = api.time().await.unwrap();
//!     println!("Kraken time: {:?}", time);
//! }
//! ```

use base64ct::{Base64, Encoding};
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256, Sha512};
use std::{convert::TryFrom, time::SystemTime};
use url::Url;

use crate::{
    AddOrderResponse, AssetPairsResponse, AssetsResponse, BalanceResponse, CancelAllOrdersAfterResponse,
    CancelAllOrdersResponse, CancelOrderResponse, DepositAddressesRequest, DepositAddressesResponse,
    DepositMethodsResponse, DepositStatusRequest, DepositStatusResponse, Error, GetOHLCDataResponse,
    GetOpenOrdersResponse, GetRecentTradesResponse, GetTradeVolumeResponse, GetWebSocketsTokenResponse,
    KrakenCredentials, KrakenRestConfig, LimitOrder, MarketOrder, OrderType, QueryOrdersResponse, Result,
    SystemStatusResponse, TickerResponse, TimeResponse, UserRefId, WithdrawAddressesResponse, WithdrawInfoRequest,
    WithdrawInfoResponse, WithdrawRequest, WithdrawResponse, WithdrawStatusRequest, WithdrawStatusResponse,
    messages::{
        AddOrderRequest, AssetPairsRequest, CancelAllOrdersAfterRequest, CancelOrderRequest, DepositMethodsRequest,
        Empty, GetOHLCDataRequest, GetOpenOrdersRequest, GetRecentTradesRequest, GetTradeVolumeRequest, KrakenResult,
        QueryOrdersRequest, TickerRequest, WithdrawAddressesRequest, unpack_kraken_result,
    },
};

// KrakenRS version
const KRAKEN_RS_VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

/// An async low-level https connection to kraken that can execute public or private methods.
pub struct KrakenRestClient {
    /// Http client
    client: reqwest::Client,
    /// Our configuration
    config: KrakenRestConfig,
    /// Base url to contact kraken at
    base_url: Url,
    /// Kraken Api version to connect to
    version: u16,
}

impl TryFrom<KrakenRestConfig> for KrakenRestClient {
    type Error = Error;
    fn try_from(config: KrakenRestConfig) -> Result<Self> {
        let base_url = Url::parse("https://api.kraken.com/")?;
        let version = 0;
        let client = reqwest::ClientBuilder::new()
            .user_agent(format!("krakenrs/{}", KRAKEN_RS_VERSION.unwrap_or("unknown")))
            .timeout(config.timeout())
            .build()?;
        Ok(Self {
            base_url,
            version,
            client,
            config,
        })
    }
}

impl KrakenRestClient {
    /// Try to create a new async KrakenRestClient from config
    ///
    /// Note: This is the same as using `TryFrom::try_from` to construct an instance
    pub fn new(config: KrakenRestConfig) -> Result<Self> {
        Self::try_from(config)
    }

    /// Change the credentials used
    pub fn set_creds(&mut self, creds: KrakenCredentials) {
        self.config.set_creds(creds);
    }

    /// Execute a public API, given method, and object matching the expected schema, and returning expected schema or an error.
    pub async fn query_public<D: Serialize, R: DeserializeOwned>(&self, method: &str, query_data: D) -> Result<R> {
        let url_path = format!("/{}/public/{}", self.version, method);

        let post_data = serde_qs::to_string(&query_data)?;

        self.query(&url_path, HeaderMap::new(), post_data).await
    }

    /// Execute a private API, given method, and object matching the expected schema, and returning expected schema or an error.
    pub async fn query_private<D: Serialize, R: DeserializeOwned>(&self, method: &str, query_data: D) -> Result<R> {
        if self.config.creds().key.is_empty() || self.config.creds().secret.is_empty() {
            return Err(Error::MissingCredentials);
        }

        let url_path = format!("/{}/private/{}", self.version, method);

        // Sign the query data and url path, resulting in encoded post_data with nonce, and a signature.
        let (post_data, sig) = self.sign(query_data, &url_path)?;

        let mut headers = HeaderMap::new();
        headers.insert("API-Key", HeaderValue::from_str(&self.config.creds().key)?);
        headers.insert("API-Sign", HeaderValue::from_str(&sig)?);

        self.query(&url_path, headers, post_data).await
    }

    /// Send a query (public or private) to kraken API, and interpret response as JSON
    async fn query<R: DeserializeOwned>(&self, url_path: &str, headers: HeaderMap, post_data: String) -> Result<R> {
        let url = self.base_url.join(url_path)?;

        let response = self.client.post(url).headers(headers).body(post_data).send().await?;
        if !(response.status() == 200 || response.status() == 201 || response.status() == 202) {
            return Err(Error::BadStatusCode(response.status().as_u16()));
        }

        let text = response.text().await?;

        let result: R = serde_json::from_str(&text).map_err(|err| Error::Json(err, text.clone()))?;
        Ok(result)
    }

    /// Serialize a json payload, adding a nonce, and producing a signature using Kraken's scheme
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

        let hmac_sha_key = Base64::decode_vec(&self.config.creds().secret).map_err(Error::SigningB64)?;

        type HmacSha = Hmac<Sha512>;
        let mut mac = HmacSha::new_from_slice(&hmac_sha_key).expect("Hmac should work with any key length");
        mac.update(url_path.as_bytes());
        mac.update(&sha2_result);
        let mac = mac.finalize().into_bytes();

        let sig = Base64::encode_string(&mac);
        Ok((post_data, sig))
    }

    /// Get a nonce as suggested by Kraken
    fn nonce() -> Result<u64> {
        Ok(SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| Error::TimeError)?
            .as_millis() as u64)
    }
}

/// An async connection to the Kraken REST API
pub struct KrakenRestAPI {
    client: KrakenRestClient,
}

impl KrakenRestAPI {
    /// Try to create async RestAPI instance from config
    pub fn new(src: KrakenRestConfig) -> Result<Self> {
        Ok(KrakenRestAPI {
            client: KrakenRestClient::try_from(src)?,
        })
    }

    /// (Public) Get the kraken system's time
    pub async fn time(&self) -> Result<TimeResponse> {
        let result: Result<KrakenResult<TimeResponse>> = self.client.query_public("Time", Empty {}).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get the kraken system's status
    pub async fn system_status(&self) -> Result<SystemStatusResponse> {
        let result: Result<KrakenResult<SystemStatusResponse>> =
            self.client.query_public("SystemStatus", Empty {}).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get the list of kraken's supported assets, and info
    pub async fn assets(&self) -> Result<AssetsResponse> {
        let result: Result<KrakenResult<AssetsResponse>> = self.client.query_public("Assets", Empty {}).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get the list of kraken's asset pairs, and info
    ///
    /// Arguments:
    /// * pairs: A list of Kraken asset pair strings to get info about. If empty then all asset pairs
    pub async fn asset_pairs(&self, pairs: Vec<String>) -> Result<AssetPairsResponse> {
        let result: Result<KrakenResult<AssetPairsResponse>> = self
            .client
            .query_public("AssetPairs", AssetPairsRequest { pair: pairs.join(",") })
            .await;
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get the ticker price for one or more asset pairs
    ///
    /// Arguments:
    /// * pairs: A list of Kraken asset pair strings to get ticker info about
    pub async fn ticker(&self, pairs: Vec<String>) -> Result<TickerResponse> {
        let result: Result<KrakenResult<TickerResponse>> = self
            .client
            .query_public("Ticker", TickerRequest { pair: pairs.join(",") })
            .await;
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
    pub async fn ohlc(&self, pair: String, since: Option<String>) -> Result<GetOHLCDataResponse> {
        let result: Result<KrakenResult<GetOHLCDataResponse>> = self
            .client
            .query_public(
                "OHLC",
                GetOHLCDataRequest {
                    pair,
                    since,
                    interval: None,
                },
            )
            .await;
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
    pub async fn ohlc_at_interval(
        &self,
        pair: String,
        interval: u16,
        since: Option<String>,
    ) -> Result<GetOHLCDataResponse> {
        let result: Result<KrakenResult<GetOHLCDataResponse>> = self
            .client
            .query_public(
                "OHLC",
                GetOHLCDataRequest {
                    pair,
                    since,
                    interval: Some(interval),
                },
            )
            .await;
        result.and_then(unpack_kraken_result)
    }

    /// (Public) Get 1000 most recent trades in an asset pair, optionally, since a particular timestamp.
    /// The response contains a "last" number which can be used as "since" to get the next page if desired.
    pub async fn get_recent_trades(&self, pair: String, since: Option<String>) -> Result<GetRecentTradesResponse> {
        let result: Result<KrakenResult<GetRecentTradesResponse>> = self
            .client
            .query_public(
                "Trades",
                GetRecentTradesRequest {
                    pair,
                    since,
                    count: None,
                },
            )
            .await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get the balance
    pub async fn get_account_balance(&self) -> Result<BalanceResponse> {
        let result: Result<KrakenResult<BalanceResponse>> = self.client.query_private("Balance", Empty {}).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get trade volume and fee tier info, per asset pair
    pub async fn get_trade_volume(&self, asset_pairs: Vec<String>) -> Result<GetTradeVolumeResponse> {
        let result: Result<KrakenResult<GetTradeVolumeResponse>> = self
            .client
            .query_private(
                "TradeVolume",
                GetTradeVolumeRequest {
                    pair: asset_pairs.join(","),
                },
            )
            .await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get a websockets authentication token
    pub async fn get_websockets_token(&self) -> Result<GetWebSocketsTokenResponse> {
        let result: Result<KrakenResult<GetWebSocketsTokenResponse>> =
            self.client.query_private("GetWebSocketsToken", Empty {}).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Query orders by order id
    pub async fn query_orders(&self, order_ids: Vec<String>) -> Result<QueryOrdersResponse> {
        let result: Result<KrakenResult<QueryOrdersResponse>> = self
            .client
            .query_private(
                "QueryOrders",
                QueryOrdersRequest {
                    txid: order_ids.join(","),
                },
            )
            .await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get the list of open orders
    ///
    /// Arguments:
    /// * userref: An optional user-reference to filter the list of open orders by
    pub async fn get_open_orders(&self, userref: Option<UserRefId>) -> Result<GetOpenOrdersResponse> {
        let result: Result<KrakenResult<GetOpenOrdersResponse>> = self
            .client
            .query_private("OpenOrders", GetOpenOrdersRequest { userref })
            .await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Cancel order
    ///
    /// Arguments:
    /// * id: A TxId (OR a UserRefId) of order(s) to cancel
    pub async fn cancel_order(&self, id: String) -> Result<CancelOrderResponse> {
        let result: Result<KrakenResult<CancelOrderResponse>> = self
            .client
            .query_private("CancelOrder", CancelOrderRequest { txid: id })
            .await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Cancel all orders (regardless of user ref or tx id)
    pub async fn cancel_all_orders(&self) -> Result<CancelAllOrdersResponse> {
        let result: Result<KrakenResult<CancelAllOrdersResponse>> =
            self.client.query_private("CancelAll", Empty {}).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Cancel all orders after
    ///
    /// Arguments:
    /// * timeout: Integer timeout specified in seconds. 0 to disable the timer.
    pub async fn cancel_all_orders_after(&self, timeout: u64) -> Result<CancelAllOrdersAfterResponse> {
        let result: Result<KrakenResult<CancelAllOrdersAfterResponse>> = self
            .client
            .query_private("CancelAllOrdersAfter", CancelAllOrdersAfterRequest { timeout })
            .await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Place a market order
    ///
    /// Arguments:
    /// * market_order: Market order object describing the parameters of the order
    /// * user_ref_id: Optional user ref id to attach to the order
    /// * validate: If true, the order is only validated and is not actually placed
    pub async fn add_market_order(
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
        let result: Result<KrakenResult<AddOrderResponse>> = self.client.query_private("AddOrder", req).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Place a limit order
    ///
    /// Arguments:
    /// * limit_order: Limit order object describing the parameters of the order
    /// * user_ref_id: Optional user ref id to attach to the order
    /// * validate: If true, the order is only validated and is not actually placed
    pub async fn add_limit_order(
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
        let result: Result<KrakenResult<AddOrderResponse>> = self.client.query_private("AddOrder", req).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get deposit methods for an asset
    ///
    /// Arguments:
    /// * asset: Asset name to get deposit methods for (e.g. "BTC")
    pub async fn get_deposit_methods(&self, asset: String) -> Result<DepositMethodsResponse> {
        let result: Result<KrakenResult<DepositMethodsResponse>> = self
            .client
            .query_private("DepositMethods", DepositMethodsRequest { asset })
            .await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get deposit addresses for an asset and method
    pub async fn get_deposit_addresses(&self, request: DepositAddressesRequest) -> Result<DepositAddressesResponse> {
        let result: Result<KrakenResult<DepositAddressesResponse>> =
            self.client.query_private("DepositAddresses", request).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get status of recent deposits
    pub async fn get_deposit_status(&self, request: DepositStatusRequest) -> Result<DepositStatusResponse> {
        let result: Result<KrakenResult<DepositStatusResponse>> =
            self.client.query_private("DepositStatus", request).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get withdrawal addresses
    ///
    /// Arguments:
    /// * asset: Optional asset to filter by (e.g. "BTC")
    /// * method: Optional withdrawal method to filter by
    pub async fn get_withdrawal_addresses(
        &self,
        asset: Option<String>,
        method: Option<String>,
    ) -> Result<WithdrawAddressesResponse> {
        let result: Result<KrakenResult<WithdrawAddressesResponse>> = self
            .client
            .query_private(
                "WithdrawAddresses",
                WithdrawAddressesRequest {
                    aclass: None,
                    asset,
                    method,
                },
            )
            .await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Withdraw funds
    pub async fn withdraw(&self, request: WithdrawRequest) -> Result<WithdrawResponse> {
        let result: Result<KrakenResult<WithdrawResponse>> = self.client.query_private("Withdraw", request).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get withdrawal fee information
    pub async fn get_withdraw_info(&self, request: WithdrawInfoRequest) -> Result<WithdrawInfoResponse> {
        let result: Result<KrakenResult<WithdrawInfoResponse>> =
            self.client.query_private("WithdrawInfo", request).await;
        result.and_then(unpack_kraken_result)
    }

    /// (Private) Get status of recent withdrawals
    pub async fn get_withdraw_status(&self, request: WithdrawStatusRequest) -> Result<WithdrawStatusResponse> {
        let result: Result<KrakenResult<WithdrawStatusResponse>> =
            self.client.query_private("WithdrawStatus", request).await;
        result.and_then(unpack_kraken_result)
    }
}

impl TryFrom<KrakenRestConfig> for KrakenRestAPI {
    type Error = Error;
    fn try_from(src: KrakenRestConfig) -> Result<Self> {
        Self::new(src)
    }
}
