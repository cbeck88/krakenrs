//! Structures and enums which are deserialized from json in the Kraken WS API
//! https://docs.kraken.com/websockets/
//!
//! Some of these are near duplicates of structures in the Kraken REST API,
//! because there are actually slight differences in the schemas and strings
//! used which make them incompatible, and the two APIs are versioned separately.

use displaydoc::Display;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_with::CommaSeparator;
use std::{collections::BTreeSet, str::FromStr};

/// Possible subscription status types in Kraken WS api
#[derive(Debug, Display, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum SubscriptionStatus {
    /// subscribed
    Subscribed,
    /// unsubscribed
    Unsubscribed,
    /// error
    Error,
}

impl SubscriptionStatus {
    /// Check if the status is equal to Subscribed
    pub fn is_subscribed(&self) -> bool {
        *self == SubscriptionStatus::Subscribed
    }
}

impl Default for SubscriptionStatus {
    fn default() -> Self {
        Self::Unsubscribed
    }
}

impl FromStr for SubscriptionStatus {
    type Err = &'static str;
    fn from_str(src: &str) -> core::result::Result<Self, Self::Err> {
        match src {
            "subscribed" => Ok(Self::Subscribed),
            "unsubscribed" => Ok(Self::Unsubscribed),
            "error" => Ok(Self::Error),
            _ => Err("unknown subscription status"),
        }
    }
}

/// Possible system status types in Kraken WS api
#[derive(Debug, Display, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum SystemStatus {
    /// online
    Online,
    /// maintenance
    Maintenance,
    /// cancel_only
    CancelOnly,
    /// limit_only
    LimitOnly,
    /// post_only
    PostOnly,
}

impl FromStr for SystemStatus {
    type Err = &'static str;
    fn from_str(src: &str) -> core::result::Result<Self, Self::Err> {
        match src {
            "online" => Ok(Self::Online),
            "maintenance" => Ok(Self::Maintenance),
            "cancel_only" => Ok(Self::CancelOnly),
            "limit_only" => Ok(Self::LimitOnly),
            "post_only" => Ok(Self::PostOnly),
            _ => Err("unknown system status"),
        }
    }
}

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

impl Default for BsType {
    fn default() -> Self {
        Self::Sell
    }
}

impl From<crate::BsType> for BsType {
    fn from(src: crate::BsType) -> Self {
        match src {
            crate::BsType::Buy => Self::Buy,
            crate::BsType::Sell => Self::Sell,
        }
    }
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

impl Default for OrderType {
    fn default() -> Self {
        Self::Limit
    }
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
    pub avg_price: Decimal,
    /// order flags (comma separated list)
    #[serde(with = "serde_with::rust::StringWithSeparator::<CommaSeparator>")]
    pub oflags: BTreeSet<OrderFlag>,
    /// misc info (comma separated list)
    #[serde(with = "serde_with::rust::StringWithSeparator::<CommaSeparator>")]
    pub misc: BTreeSet<MiscInfo>,
}

/// Possible order flags in Kraken WS API.
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

impl From<crate::OrderFlag> for OrderFlag {
    fn from(src: crate::OrderFlag) -> Self {
        match src {
            crate::OrderFlag::Post => Self::Post,
            crate::OrderFlag::Fcib => Self::Fcib,
            crate::OrderFlag::Fciq => Self::Fciq,
            crate::OrderFlag::Nompp => Self::Nompp,
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
    pub leverage: Option<Decimal>,
    /// human-readable description
    pub order: String,
}

/// Add order request (websockets)
#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddOrderRequest {
    /// The event will be "addOrder"
    pub event: String,
    /// The token used to authenticate
    pub token: String,
    /// A req-id associated to the order
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reqid: Option<u64>,
    /// A user ref id for this order
    #[serde(with = "serde_with::rust::display_fromstr")]
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
    #[serde(with = "serde_with::rust::display_fromstr")]
    #[serde(skip_serializing_if = "core::ops::Not::not")]
    pub validate: bool,
}
