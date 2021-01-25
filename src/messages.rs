//! Structures representing json schema sent to and from Kraken

use crate::{Error, Result};
use displaydoc::Display;
use serde::{Deserialize, Serialize};
use serde_with::CommaSeparator;
use std::collections::{BTreeSet, HashMap};
use std::str::FromStr;

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
pub struct TimeResponse {
    /// Unix time stamp (seconds since epoch)
    pub unixtime: u64,
}

/// Result of kraken public "SystemStatus" API call
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatusResponse {
    /// Status of kraken's trading system
    pub status: SystemStatus,
    /// Time that this was the status (format: 2021-01-20T20:39:22Z)
    pub timestamp: String,
}

/// A possible status of the kraken trading system
#[derive(Debug, Display, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SystemStatus {
    /// Online
    Online,
    /// Cancel Only (new orders cannot be created)
    CancelOnly,
    /// Post Only (only new post limit orders can be created)
    PostOnly,
    /// Limit Only (only new limit orders can be created)
    LimitOnly,
    /// Mainanence (system is offline for maintenance)
    Maintenance,
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

/// Type alias for response of Assets API call
pub type AssetsResponse = HashMap<String, AssetInfo>;

/// Type alias for response of Balance API call
pub type BalanceResponse = HashMap<String, String>;

/// TxId are represented as String's in kraken json api
pub type TxId = String;

/// User-reference id's are signed 32-bit in kraken json api
pub type UserRefId = i32;

/// Type (buy/sell)
/// These are kebab-case strings in json
#[derive(Debug, Display, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Type {
    /// Buy
    Buy,
    /// Sell
    Sell,
}

/// Possible order types in Kraken.
/// These are kebab-case strings in json
#[derive(Debug, Display, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum OrderType {
    /// Market
    Market,
    /// Limit
    Limit,
    /// Stop-Loss
    StopLoss,
    /// Take-Profit
    TakeProfit,
    /// Stop-Loss-Limit
    StopLossLimit,
    /// Take-Profit-Limit
    TakeProfitLimit,
    /// Settle-Position
    SettlePosition,
}

/// Possible order statuses in Kraken.
/// These are kebab-case strings in json
#[derive(Debug, Display, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum OrderStatus {
    /// Pending
    Pending,
    /// Open
    Open,
    /// Closed
    Closed,
    /// Canceled
    Canceled,
    /// Expired
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
    /// order flags (comma separated list)
    #[serde(with = "serde_with::rust::StringWithSeparator::<CommaSeparator>")]
    pub oflags: BTreeSet<OrderFlag>,
    /// misc info (comma separated list)
    #[serde(with = "serde_with::rust::StringWithSeparator::<CommaSeparator>")]
    pub misc: BTreeSet<MiscInfo>,
}

/// Possible order flags in Kraken.
/// These are options in a comma-separated list
#[derive(Debug, Display, Ord, PartialOrd, Eq, PartialEq)]
pub enum OrderFlag {
    /// viqc
    Viqc,
    /// fcib
    Fcib,
    /// fciq
    Fciq,
    /// nompp
    Nompp,
}

impl FromStr for OrderFlag {
    type Err = &'static str;
    fn from_str(src: &str) -> core::result::Result<OrderFlag, Self::Err> {
        match src {
            "viqc" => Ok(OrderFlag::Viqc),
            "fcib" => Ok(OrderFlag::Fcib),
            "fciq" => Ok(OrderFlag::Fciq),
            "nompp" => Ok(OrderFlag::Nompp),
            _ => Err("unknown OrderFlag"),
        }
    }
}

/// Possible miscellaneous info flags in Kraken.
/// These are options in a comma-separated list
#[derive(Debug, Display, Ord, PartialOrd, Eq, PartialEq)]
pub enum MiscInfo {
    /// stopped
    Stopped,
    /// touched
    Touched,
    /// liquidated
    Liquidated,
    /// partial
    PartialFill,
}

impl FromStr for MiscInfo {
    type Err = &'static str;
    fn from_str(src: &str) -> core::result::Result<MiscInfo, Self::Err> {
        match src {
            "stopped" => Ok(MiscInfo::Stopped),
            "touched" => Ok(MiscInfo::Touched),
            "liquidated" => Ok(MiscInfo::Liquidated),
            "partial" => Ok(MiscInfo::PartialFill),
            _ => Err("unknown MiscInfo"),
        }
    }
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

/// Cancel order request
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct CancelOrderRequest {
    /// The txid of the order to cancel. OR a userref id of orders to cancel
    pub txid: String,
}

/// Cancel order response
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct CancelOrderResponse {
    /// The number of orders canceled
    pub count: u64,
    /// if set, order(s) is/are pending cancellation
    #[serde(default)]
    pub pending: bool,
}

/// Cancel all orders response
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct CancelAllOrdersResponse {
    /// The number of orders canceled
    pub count: u64,
}

/// Cancel all orders after request
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct CancelAllOrdersAfterRequest {
    /// The timeout in seconds until all orders are canceled, unless the trigger is set again before then. 0 disables.
    pub timeout: u64,
}

/// Cancel all orders after response
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct CancelAllOrdersAfterResponse {
    /// The time when the request is handled (RFC 3339)
    #[serde(rename = "currentTime")]
    pub current_time: String,
    /// The time when the trigger is set for (RFC 3339)
    #[serde(rename = "triggerTime")]
    pub trigger_time: String,
}
