//! Structures representing json schema sent to and from Kraken REST API
//! <https://docs.kraken.com/rest/>

use crate::{Error, LastAndData, Result};
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
#[derive(Debug, Deserialize)]
pub struct KrakenResult<ResultJson> {
    /// Kraken API returns error strings in an array marked "error"
    pub error: Vec<String>,
    /// Kraken API returns results here, separated from error
    /// Sometimes result is omitted if errors occured.
    pub result: Option<ResultJson>,
}

/// Convert KrakenResult<T> to Result<T>
pub fn unpack_kraken_result<ResultJson>(src: KrakenResult<ResultJson>) -> Result<ResultJson> {
    if !src.error.is_empty() {
        return Err(Error::KrakenErrors(src.error));
    }
    src.result.ok_or(Error::MissingResultJson)
}

/// Empty json object (used as arguments for some APIs)
#[derive(Debug, Serialize, Deserialize)]
pub struct Empty {}

/// Result of kraken public "Time" API call
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimeResponse {
    /// Unix time stamp (seconds since epoch)
    pub unixtime: u64,
}

/// Result of kraken public "SystemStatus" API call
#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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

/// A query object to kraken public "Get Recent Trades" API call
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct GetRecentTradesRequest {
    /// A comma-separated list of kraken asset pair strings
    pub pair: String,
    /// Return trade data since given timestamp
    pub since: Option<String>,
    /// Return a specific number of trades, up to 1000.
    /// Defaults to 1000.
    pub count: Option<usize>,
}

/// Response object of Get Recent Trades API call
/// (Note: See issue #3 for discussion of strategy)
pub type GetRecentTradesResponse = LastAndData<Vec<PublicTrade>>;

/// A sub-object of the recent-trades response
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(expecting = "expecting [<price>, <volume>, <timestamp>, <buy/sell>, <order_type>, <misc>, <trade_id>] array")]
pub struct PublicTrade {
    /// The price at which the trade took place
    pub price: Decimal,
    /// The volume of the trade
    pub volume: Decimal,
    /// The timestamp of the trade (seconds since the unix epoch)
    #[serde(deserialize_with = "rust_decimal::serde::arbitrary_precision::deserialize")]
    pub timestamp: Decimal,
    /// Whether it was a buy or a sell
    pub bs_type: BsType,
    /// The order type of the trade (market or limit)
    pub order_type: OrderType,
    /// Misc (always empty at time of writing)
    pub misc: String,
    /// The trade id (an incrementing counter identifying trades)
    /// Note that this isn't visible in the websockets v1 trade feed interface
    pub trade_id: u64,
}

/// A query object to kraken public "Get OHLC Data" API call
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct GetOHLCDataRequest {
    /// An asset pair
    pub pair: String,
    /// Return ohlc data since given timestamp
    pub since: Option<String>,
    /// A number of minutes for the width of each candle. Defaults to 1 minute.
    /// Allowed values are:
    /// 1, 5, 15, 30, 60, 240, 1440, 10080, 21600
    pub interval: Option<u16>,
}

/// Response object of Get OHLC data API call
/// (Note: See issue #3 for discussion of strategy)
pub type GetOHLCDataResponse = LastAndData<Vec<Candle>>;

/// A sub-object of the OHLC data response
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(expecting = "expecting [<timestamp>, <open>, <high>, <low>, <close>, <vwap>, <volume>, <trades>] array")]
pub struct Candle {
    /// The timestamp of the candle (seconds since the unix epoch)
    #[serde(deserialize_with = "rust_decimal::serde::arbitrary_precision::deserialize")]
    pub timestamp: Decimal,
    /// The price at the open of the candle period
    pub open: Decimal,
    /// The highest price during the candle period
    pub high: Decimal,
    /// The lowest price during the candle period
    pub low: Decimal,
    /// The price at the end of the candle period
    pub close: Decimal,
    /// The volume-weighted average price during the candle period
    pub vwap: Decimal,
    /// The volume during the candle period
    pub volume: Decimal,
    /// The total number of trades during the candle period
    pub trades: usize,
}

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
    #[serde(alias = "b")]
    Buy,
    /// Sell
    #[serde(alias = "s")]
    Sell,
}

/// Possible order types in Kraken.
/// These are kebab-case strings in json
#[derive(Debug, Display, Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum OrderType {
    /// Market
    #[serde(alias = "m")]
    Market,
    /// Limit
    #[serde(alias = "l")]
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
    /// viqc
    Viqc,
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
            "viqc" => Ok(OrderFlag::Viqc),
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

/// Query orders request schema
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct QueryOrdersRequest {
    pub txid: String,
}

/// Query orders response schema, keyed by tx id
pub type QueryOrdersResponse = HashMap<String, OrderInfo>;

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

/// GetTradeVolume request
#[derive(Debug, Serialize, Deserialize)]
pub struct GetTradeVolumeRequest {
    /// Comma-separated asset pairs to get fee info for
    pub pair: String,
}

/// GetTradeVolume response
#[derive(Debug, Serialize, Deserialize)]
pub struct GetTradeVolumeResponse {
    /// Total 30-day volume this account is credited for
    pub volume: Decimal,
    /// Taker fees, per asset pair requested
    pub fees: HashMap<String, FeeTierInfo>,
    /// Maker fees, per asset pair requested
    pub fees_maker: HashMap<String, FeeTierInfo>,
}

/// Substructure of GetTradeVolume response
#[derive(Debug, Serialize, Deserialize)]
pub struct FeeTierInfo {
    /// Fee, expressed as a %
    pub fee: Decimal,
}

/// WebSockets authenitcation token response, including token and expiry
#[derive(Debug, Serialize, Deserialize)]
pub struct GetWebSocketsTokenResponse {
    /// Websockets authentication token
    pub token: String,
    /// Expiration time (in seconds)
    pub expires: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_trade() {
        let text = r#"["314.11000","0.38203178",2,"b","l","",4151536]"#;

        let obj: PublicTrade = serde_json::from_str(text).unwrap();

        assert_eq!(obj.price, Decimal::new(31411, 2));
        assert_eq!(obj.volume, Decimal::new(38203178, 8));
        assert_eq!(obj.timestamp, Decimal::new(2, 0));
        assert_eq!(obj.bs_type, BsType::Buy);
        assert_eq!(obj.order_type, OrderType::Limit);
        assert_eq!(obj.misc, "");
        assert_eq!(obj.trade_id, 4151536);
    }

    #[test]
    fn test_public_trade2() {
        let text = r#"["314.11000","0.38203178",1756443751.1748989,"b","l","",4151536]"#;

        let obj: PublicTrade = serde_json::from_str(text).unwrap();

        assert_eq!(obj.price, Decimal::new(31411, 2));
        assert_eq!(obj.volume, Decimal::new(38203178, 8));
        assert_eq!(obj.timestamp.trunc(), Decimal::new(1756443751, 0));
        assert_eq!(obj.timestamp.fract(), Decimal::new(1748989, 7));
        assert_eq!(obj.bs_type, BsType::Buy);
        assert_eq!(obj.order_type, OrderType::Limit);
        assert_eq!(obj.misc, "");
        assert_eq!(obj.trade_id, 4151536);
    }

    #[test]
    fn test_get_recent_trades_response() {
        // This text obtained as
        // `curl "https://api.kraken.com/0/public/Trades?pair=AAVEUSD&count=3"`
        let text = r#"{"AAVEUSD":[["314.11000","0.38203178",1756443751.1748989,"b","l","",4151536],["314.01000","0.26532000",1756443816.201052,"b","l","",4151537],["314.01000","0.24987835",1756443816.201052,"b","l","",4151538]],"last":"1756443816201051892"}"#;

        let obj: GetRecentTradesResponse = serde_json::from_str(text).unwrap();

        assert_eq!(obj.data.len(), 3);
        assert_eq!(obj.last, "1756443816201051892");
    }

    #[test]
    fn test_get_ohlc_data_response() {
        // This text from kraken docs api console
        let text = r#"{"BTC/USD": [[1756880160,"110680.0","110680.0","110679.9","110680.0","110679.9","0.09192328",12],[1756880220,"110680.0","110680.0","110680.0","110680.0","110680.0","7.19286980",22],[1756880280,"110680.0","110691.1","110680.0","110691.1","110680.0","0.67532130",16],[1756880340,"110691.1","110716.6","110691.1","110716.5","110708.2","0.02984317",13],[1756880400,"110724.8","110724.9","110724.8","110724.9","110724.8","0.00802704",3],[1756880460,"110734.9","110739.0","110734.9","110739.0","110735.4","0.01682744",6],[1756880520,"110759.1","110759.1","110759.0","110759.0","110759.0","0.02548629",2],[1756880580,"110759.1","110775.1","110759.1","110775.1","110766.5","0.01183179",10],[1756880640,"110775.1","110779.7","110775.0","110779.6","110779.1","0.04210282",11]],"last": 1756923300}"#;

        let obj: GetOHLCDataResponse = serde_json::from_str(text).unwrap();

        assert_eq!(obj.data.len(), 9);
        assert_eq!(obj.last, "1756923300");
    }
}
