//! Structures and enums which are deserialized from json in the Kraken WS API
//! <https://docs.kraken.com/websockets/>
//!
//! Some of these are near duplicates of structures in the Kraken REST API,
//! because there are actually slight differences in the schemas and strings
//! used which make them incompatible, and the two APIs are versioned separately.

use crate::serde_helpers::{comma_separated, display_fromstr};
use displaydoc::Display;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, Serializer};
use std::{collections::BTreeSet, str::FromStr};

/// Possible subscription status types in Kraken WS api
#[derive(Debug, Default, Display, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum SubscriptionStatus {
    /// subscribed
    Subscribed,
    /// unsubscribed
    #[default]
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
#[derive(Debug, Default, Display, Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum BsType {
    /// Buy
    Buy,
    /// Sell
    #[default]
    Sell,
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
#[derive(Debug, Default, Display, Clone, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum OrderType {
    /// Market
    Market,
    /// Limit
    #[default]
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

/// Partial order update sent by Kraken WS API for fill updates.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderInfoPartialUpdate {
    /// User reference id for the order
    pub userref: UserRefId,
    /// volume executed (base currency unless viqc set in oflags)
    pub vol_exec: Decimal,
    /// total cost (quote currency unless unless viqc set in oflags)
    pub cost: Decimal,
    /// total fee (quote currency)
    pub fee: Decimal,
    /// average price (quote currency unless viqc set in oflags)
    pub avg_price: Decimal,
    /// order flags (comma separated list)
    #[serde(with = "comma_separated")]
    pub oflags: BTreeSet<OrderFlag>,
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
    #[serde(with = "comma_separated")]
    pub oflags: BTreeSet<OrderFlag>,
    /// misc info (comma separated list)
    #[serde(with = "comma_separated")]
    pub misc: BTreeSet<MiscInfo>,
}

/// Possible order flags in Kraken WS API.
/// These are options in a comma-separated list
///
/// * post: Post-only (only for limit orders. Prevents immediately matching as a market order)
/// * fcib: Prefer fee in base currency. Default when selling.
/// * fciq: Prefer fee in quote currency. Default when buying.
/// * viqc: Volume in quote currency.
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

impl From<crate::OrderFlag> for OrderFlag {
    fn from(src: crate::OrderFlag) -> Self {
        match src {
            crate::OrderFlag::Post => Self::Post,
            crate::OrderFlag::Fcib => Self::Fcib,
            crate::OrderFlag::Fciq => Self::Fciq,
            crate::OrderFlag::Viqc => Self::Viqc,
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
    #[serde(serialize_with = "user_ref_ser")]
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
    #[serde(with = "comma_separated")]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub oflags: BTreeSet<OrderFlag>,
    /// validate: If true, do not submit order
    #[serde(skip_serializing_if = "core::ops::Not::not")]
    #[serde(with = "display_fromstr")]
    pub validate: bool,
}

fn user_ref_ser<S>(src: &Option<UserRefId>, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let text = if let Some(src) = src {
        src.to_string()
    } else {
        Default::default()
    };
    text.serialize(ser)
}

/// A record of one of our own trades, from the ownTrades feed (websockets)
#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OwnTrade {
    /// the kraken unique id for this trade
    // must allow defaulted, because kraken stores this as the key of this object rather than in the object
    #[serde(default)]
    pub trade_id: String,
    /// the unique id for the order this trade corresponds to
    pub ordertxid: String,
    /// the asset pair the trade was made in
    pub pair: String,
    /// type of trade (buy/sell)
    #[serde(rename = "type")]
    pub bs_type: BsType,
    /// type of order (market, limit etc.)
    pub ordertype: OrderType,
    /// time of the trade (unix timestamp)
    pub time: Decimal,
    /// average price of the trade
    pub price: Decimal,
    /// total volume of the trade (base currency quantity)
    pub vol: Decimal,
    /// total cost of the trade (quote currency quantity)
    pub cost: Decimal,
    /// total fees in the quote currency
    pub fee: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_own_trade_record() {
        let json = r#"{
        "cost": "1000000.00000",
        "fee": "1600.00000",
        "margin": "0.00000",
        "ordertxid": "TDLH43-DVQXD-2KHVYY",
        "ordertype": "limit",
        "pair": "XBT/EUR",
        "postxid": "OGTT3Y-C6I3P-XRI6HX",
        "price": "100000.00000",
        "time": "1560516023.070651",
        "type": "sell",
        "vol": "1000000000.00000000"
      }"#;
        let val: OwnTrade = serde_json::from_str(json).unwrap();

        assert_eq!(val.pair, "XBT/EUR");
        assert_eq!(val.bs_type, BsType::Sell);
    }
}
