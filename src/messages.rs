//! Structures representing json schema sent to and from Kraken REST API
//! https://docs.kraken.com/rest/

use crate::{Error, Result};
use displaydoc::Display;
use rust_decimal::Decimal;
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
pub fn unpack_kraken_result<ResultJson: Serialize>(src: KrakenResult<ResultJson>) -> Result<ResultJson> {
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
#[derive(Debug, Display, Serialize, Deserialize, Ord, Clone, PartialOrd, Eq, PartialEq)]
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

/// A query object to kraken public "AssetPairs" API call
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct AssetPairsRequest {
    /// A comma-separated list of kraken asset pair strings
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pair: String,
}

/// (Substructure within) Result of kraken public "Asset Pairs" API call
#[derive(Debug, Serialize, Deserialize)]
pub struct AssetPair {
    /// Alternate pair name
    pub alt_name: Option<String>,
    /// Web-sockets pair name (if available)
    pub wsname: Option<String>,
    /// Asset class of base component
    pub aclass_base: String,
    /// Asset id of base component
    pub base: String,
    /// Asset class of quote component
    pub aclass_quote: String,
    /// Asset id of quote component
    pub quote: String,
    /// Scaling decimal places for pair
    pub pair_decimals: u64,
    /// Scaling decimal places for volume
    pub lot_decimals: u64,
    /// Amount to multiply lot volume by to get currency volume
    pub lot_multiplier: u64,
    /// Fee schedule array in [volume, percent] tuples
    pub fees: Vec<Vec<Decimal>>,
    /// Minimum order size (in terms of base currency)
    pub ordermin: Option<Decimal>,
}

/// Type alias for response of AssetPairs API call
pub type AssetPairsResponse = HashMap<String, AssetPair>;

/// A query object to kraken public "Ticker" API call
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct TickerRequest {
    /// A comma-separated list of kraken asset pair strings
    pub pair: String,
}

/// (Substructure within) Result of kraken public "Ticker" API call
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AssetTickerInfo {
    /// Ask `[price, whole lot volume, lot volume]`
    pub a: Vec<String>,
    /// Bid `[price, whole lot volume, lot volume]`
    pub b: Vec<String>,
    /// Closed `[price, lot volume]`
    pub c: Vec<String>,
}

/// Type alias for response of Ticker API call
pub type TickerResponse = HashMap<String, AssetTickerInfo>;

/// Type alias for response of Balance API call
pub type BalanceResponse = HashMap<String, Decimal>;

/// TxId are represented as String's in kraken json api
pub type TxId = String;

/// User-reference id's are signed 32-bit in kraken json api
pub type UserRefId = i32;

/// Type (buy/sell)
/// These are kebab-case strings in json
#[derive(Debug, Display, Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum BsType {
    /// Buy
    Buy,
    /// Sell
    Sell,
}

/// Possible order types in Kraken.
/// These are kebab-case strings in json
#[derive(Debug, Display, Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
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
#[derive(Debug, Display, Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderInfo {
    /// User reference id for the order
    pub userref: UserRefId,
    /// Status of the order
    pub status: OrderStatus,
    /// unix timestamp of when the order was placed
    pub opentm: Decimal,
    /// unix timestamp of order start time
    pub starttm: Option<Decimal>,
    /// unix timestamp of order end time
    pub expiretm: Option<Decimal>,
    /// order description info
    pub descr: OrderDescriptionInfo,
    /// volume of order (base currency unless viqc set in oflags)
    pub vol: Decimal,
    /// volume executed (base currency unless viqc set in oflags)
    pub vol_exec: Decimal,
    /// total cost (quote currency unless unless viqc set in oflags)
    pub cost: Decimal,
    /// total fee (quote currency)
    pub fee: Decimal,
    /// average price (quote currency unless viqc set in oflags)
    pub price: Decimal,
    /// order flags (comma separated list)
    #[serde(with = "serde_with::rust::StringWithSeparator::<CommaSeparator>")]
    pub oflags: BTreeSet<OrderFlag>,
    /// misc info (comma separated list)
    #[serde(with = "serde_with::rust::StringWithSeparator::<CommaSeparator>")]
    pub misc: BTreeSet<MiscInfo>,
}

/// Possible order flags in Kraken.
/// These are options in a comma-separated list
///
/// * post: Post-only (only for limit orders. Prevents immediately matching as a market order)
/// * fcib: Prefer fee in base currency. Default when selling.
/// * fciq: Prefer fee in quote currency. Default when buying.
/// * nompp: Disable market order protection.
#[derive(Debug, Display, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum OrderFlag {
    /// post
    Post,
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
            "post" => Ok(OrderFlag::Post),
            "fcib" => Ok(OrderFlag::Fcib),
            "fciq" => Ok(OrderFlag::Fciq),
            "nompp" => Ok(OrderFlag::Nompp),
            _ => Err("unknown OrderFlag"),
        }
    }
}

/// Possible miscellaneous info flags in Kraken.
/// These are options in a comma-separated list
#[derive(Debug, Display, Clone, Ord, PartialOrd, Eq, PartialEq)]
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

/// Order-description-info used in GetOpenOrders API
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderDescriptionInfo {
    /// asset pair
    pub pair: String,
    /// type of order (buy/sell)
    #[serde(rename = "type")]
    pub bs_type: BsType,
    /// order type
    pub ordertype: OrderType,
    /// primary price
    pub price: Decimal,
    /// secondary price
    pub price2: Decimal,
    /// leverage
    #[serde(deserialize_with = "serde_with::rust::default_on_error::deserialize")]
    pub leverage: Option<Decimal>,
    /// human-readable description
    pub order: String,
}

/// Get open orders request
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct GetOpenOrdersRequest {
    /// restrict results to given user reference id (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
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

/// Add order request
#[derive(Debug, Serialize, Deserialize)]
pub struct AddOrderRequest {
    /// A user ref id for this order
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userref: Option<UserRefId>,
    /// order type
    pub ordertype: OrderType,
    /// type of order (buy/sell)
    #[serde(rename = "type")]
    pub bs_type: BsType,
    /// volume (in lots)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub volume: String,
    /// pair (AssetPair id or altname)
    pub pair: String,
    /// price
    #[serde(skip_serializing_if = "String::is_empty")]
    pub price: String,
    /// order flags (comma separated list)
    #[serde(with = "serde_with::rust::StringWithSeparator::<CommaSeparator>")]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub oflags: BTreeSet<OrderFlag>,
    /// validate: If true, do not submit order
    #[serde(skip_serializing_if = "core::ops::Not::not")]
    pub validate: bool,
}

/// Add order response
#[derive(Debug, Serialize, Deserialize)]
pub struct AddOrderResponse {
    /// Description of resulting order
    pub descr: OrderAdded,
    /// Txids associated to order, if successful
    #[serde(default)]
    pub txid: Vec<String>,
}

/// Substructure within AddOrderResponse
#[derive(Debug, Serialize, Deserialize)]
pub struct OrderAdded {
    /// Human-readable description of the order
    #[serde(default)]
    pub order: String,
    /// Conditional close order description, if applicable
    #[serde(default)]
    pub close: String,
}

/// WebSockets authenitcation token response, including token and expiry
#[derive(Debug, Serialize, Deserialize)]
pub struct GetWebSocketsTokenResponse {
    /// Websockets authentication token
    pub token: String,
    /// Expiration time (in seconds)
    pub expires: u64,
}
