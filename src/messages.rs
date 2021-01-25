//! Structures representing json schema sent to and from Kraken

use crate::{Error, Result};
use displaydoc::Display;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Kraken responds to APIs with a json body consisting of "error:" and "result:" fields.
/// The error part is an array of strings encoded as:
/// <char-severity code><string-error category>:<string-error type>[:<string-extra info>]
/// The result part is a json object or array
#[derive(Debug, Serialize, Deserialize)]
pub struct KrakenResult<ResultJson: Serialize> {
    /// Kraken API returns error strings in an array marked "error"
    pub error: Vec<String>,
    /// Kraken API returns results here, separated from error
    /// Sometimes result is omitted if errors occured.
    pub result: Option<ResultJson>,
}

/// Convert KrakenResult<T> to Result<T>
pub fn unpack_kraken_result<ResultJson: Serialize>(
    src: KrakenResult<ResultJson>,
) -> Result<ResultJson> {
    if !src.error.is_empty() {
        return Err(Error::KrakenErrors(src.error));
    }
    src.result.ok_or(Error::MissingResultJson)
}

/// Empty json object (used as arguments for some APIs)
#[derive(Debug, Serialize, Deserialize)]
pub struct Empty {}

/// Result of kraken public "Time" API call
#[derive(Debug, Serialize, Deserialize)]
pub struct Time {
    /// Unix time stamp (seconds since epoch)
    pub unixtime: u64,
}

/// Result of kraken public "SystemStatus" API call
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    /// Status of kraken's trading system
    pub status: String,
    /// Time that this was the status (format: 2021-01-20T20:39:22Z)
    pub timestamp: String,
}

/// (Substructure within) Result of kraken public "Assets" API call
#[derive(Debug, Serialize, Deserialize)]
pub struct AssetInfo {
    /// Alternative name for the asset
    pub altname: String,
    /// The asset class
    pub aclass: String,
    /// scaling decimal places for record keeping
    pub decimals: u32,
    /// scaling decimal places for output display
    pub display_decimals: u32,
}

/// TxId are represented as String's in kraken json api
pub type TxId = String;

/// User-reference id's are signed 32-bit in kraken json api
pub type UserRefId = i32;

/// Type (buy/sell)
/// These are kebab-case strings in json
#[derive(Debug, Display, Serialize, Deserialize)]
pub enum Type {
    /// Buy
    #[serde(rename = "buy")]
    Buy,
    /// Sell
    #[serde(rename = "sell")]
    Sell,
}

/// Possible order types in Kraken.
/// These are kebab-case strings in json
#[derive(Debug, Display, Serialize, Deserialize)]
pub enum OrderType {
    /// Market
    #[serde(rename = "market")]
    Market,
    /// Limit
    #[serde(rename = "limit")]
    Limit,
    /// Stop-Loss
    #[serde(rename = "stop-loss")]
    StopLoss,
    /// Take-Profit
    #[serde(rename = "take-profit")]
    TakeProfit,
    /// Stop-Loss-Limit
    #[serde(rename = "stop-loss-limit")]
    StopLossLimit,
    /// Take-Profit-Limit
    #[serde(rename = "take-profit-limit")]
    TakeProfitLimit,
    /// Settle-Position
    #[serde(rename = "settle-position")]
    SettlePosition,
}

/// Possible order statuses in Kraken.
/// These are kebab-case strings in json
#[derive(Debug, Display, Serialize, Deserialize)]
pub enum OrderStatus {
    /// Pending
    #[serde(rename = "pending")]
    Pending,
    /// Open
    #[serde(rename = "open")]
    Open,
    /// Closed
    #[serde(rename = "closed")]
    Closed,
    /// Canceled
    #[serde(rename = "canceled")]
    Canceled,
    /// Expired
    #[serde(rename = "expired")]
    Expired,
}

/// Order-info used in OpenOrders and QueryOrders APIs
#[derive(Debug, Serialize, Deserialize)]
pub struct OrderInfo {
    /// User reference id for the order
    pub userref: UserRefId,
    /// Status of the order
    pub status: OrderStatus,
    /// unix timestamp of when the order was placed
    pub opentm: f64,
    /// unix timestamp of order start time (or 0 if not set)
    #[serde(default)]
    pub starttm: f64,
    /// unix timestamp of order end time (or 0 if not set)
    #[serde(default)]
    pub expiretm: f64,
    /// order description info
    pub descr: OrderDescriptionInfo,
    /// volume of order (base currency unless viqc set in oflags)
    pub vol: String,
    /// volume executed (base currency unless viqc set in oflags)
    pub vol_exec: String,
    /// total cost (quote currency unless unless viqc set in oflags)
    pub cost: String,
    /// total fee (quote currency)
    pub fee: String,
    /// average price (quote currency unless viqc set in oflags)
    pub price: String,
}

/// Order-description-info used in Order APIs and AddStandardOrder API
#[derive(Debug, Serialize, Deserialize)]
pub struct OrderDescriptionInfo {
    /// asset pair
    pub pair: String,
    /// type of order (buy/sell)
    #[serde(rename = "type")]
    pub bs_type: Type,
    /// order type
    pub ordertype: OrderType,
    /// primary price
    pub price: String,
    /// secondary price
    pub price2: String,
    /// leverage
    pub leverage: String,
    /// human-readable description
    pub order: String,
}

/// Get open orders request
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct GetOpenOrdersRequest {
    /// restrict results to given user reference id (optional)
    pub userref: Option<UserRefId>,
}

/// Get open orders response
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct GetOpenOrdersResponse {
    /// The set of open orders, keyed by TxId
    pub open: HashMap<TxId, OrderInfo>,
}
