use super::messages::BsType;
use displaydoc::Display;
use rust_decimal::Decimal;
use serde_json::Value;
use std::{collections::BTreeMap, str::FromStr, time::Instant};

/// The state of the book for some asset pair
#[derive(Default, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct BookData {
    /// The current asks, sorted by price
    pub ask: BTreeMap<Decimal, BookEntry>,
    /// The current bids, sorted by price
    pub bid: BTreeMap<Decimal, BookEntry>,
    /// Indicates that the book data is invalid
    pub checksum_failed: bool,
    /// When the book was last updated (if ever)
    pub last_update: Option<Instant>,
}

impl BookData {
    /// Clear the book. This happens when we receive a snapshot
    pub fn clear(&mut self) {
        *self = Default::default();
    }

    /// Compute the book checksum according to Kraken's algorithm
    pub fn checksum(&self) -> u32 {
        let mut hasher = crc32fast::Hasher::new();
        // asks must be sorted low to high
        for (_, ask) in self.ask.iter().take(10) {
            ask.crc32(&mut hasher);
        }
        // bids must be sorted high to low
        for (_, bid) in self.bid.iter().rev().take(10) {
            bid.crc32(&mut hasher);
        }
        hasher.finalize()
    }

    /// Update the ask side
    pub fn update_asks(&mut self, data: &Value, depth: usize) -> Result<(), &'static str> {
        Self::update_internal(&mut self.ask, data)?;
        if self.ask.len() > depth {
            let mut count = 0;
            // Keep only the first "depth" many entries
            self.ask.retain(|_, _| {
                count += 1;
                count <= depth
            });
        }
        Ok(())
    }

    /// Update the bid side
    pub fn update_bids(&mut self, data: &Value, depth: usize) -> Result<(), &'static str> {
        Self::update_internal(&mut self.bid, data)?;
        let len = self.bid.len();
        if len > depth {
            let mut count = 0;
            // Keep only the last "depth" many entries
            self.bid.retain(|_, _| {
                count += 1;
                count >= (len - depth + 1)
            });
        }
        Ok(())
    }

    // Shared code between update_asks and update_bids
    fn update_internal(side: &mut BTreeMap<Decimal, BookEntry>, data: &Value) -> Result<(), &'static str> {
        let outer_array = data.as_array().ok_or("update was not a json array")?;
        for data in outer_array.iter() {
            let data = data.as_array().ok_or("update did not contain a json array")?;
            let price_level_str = data[0].as_str().ok_or("price level was not a json string")?;
            let volume_str = data[1].as_str().ok_or("volume was not a json string")?;
            let timestamp_str = data[2].as_str().ok_or("timestamp was not a json string")?;

            let price_level = Decimal::from_str(price_level_str).map_err(|_| "could not parse price level")?;
            let volume = Decimal::from_str(volume_str).map_err(|_| "could not parse volume")?;
            let timestamp = Decimal::from_str(timestamp_str).map_err(|_| "could not parse timestamp")?;

            if volume == Decimal::ZERO {
                side.remove(&price_level);
            } else {
                side.insert(
                    price_level,
                    BookEntry {
                        volume,
                        timestamp,
                        price_str: price_level_str.to_string(),
                        volume_str: volume_str.to_string(),
                    },
                );
            }
        }
        Ok(())
    }
}

/// An entry in an order book
#[derive(Default, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct BookEntry {
    /// The volume of this book entry
    pub volume: Decimal,
    /// The timestamp of this of this book entry (Decimal) (seconds since epoch)
    pub timestamp: Decimal,
    /// The price of this book entry (Decimal), for computing checksum
    pub price_str: String,
    /// The volume of this book entry (Decimal), for computing checksum
    pub volume_str: String,
}

impl BookEntry {
    fn crc32(&self, hasher: &mut crc32fast::Hasher) {
        hasher.update(Self::format_str_for_hash(&self.price_str).as_bytes());
        hasher.update(Self::format_str_for_hash(&self.volume_str).as_bytes());
    }
    fn format_str_for_hash(arg: &str) -> String {
        let remove_decimal: String = arg.chars().filter(|x| *x != '.').collect();
        let first_nonzero = remove_decimal
            .chars()
            .position(|x| x != '0')
            .unwrap_or(remove_decimal.len());
        remove_decimal[first_nonzero..].to_string()
    }
}

/// A record of a public trade
#[derive(Default, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct PublicTrade {
    /// The price at which this trade took place
    pub price: Decimal,
    /// The volume of this trade
    pub volume: Decimal,
    /// The side (taker side) of this trade
    pub side: BsType,
    /// The timestamp of this of this trade (Decimal) (seconds since epoch)
    pub timestamp: Decimal,
}

/// A candle record contains information about price activity during an "epoch".
/// The interval is set when the feed is subscribed to, and determines the duration
/// of the epoch in minutes. The candle record includes the end timestamp of the epoch,
/// the highest and lowest price levels observed during the epoch, the open and close
/// prices during the epoch, etc.
///
/// Kraken sends "partial" candle records, so not every candle record indicates
/// the final values for that epoch. Multiple candles may be received with the same
/// `epoc_end` but increasing values of `epoc_last`.
/// The last candle record received with a given value of `epoc_end` indicates the final candle values for that epoch.
#[derive(Default, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub struct Candle {
    /// Last update time of the candle (seconds since epoch)
    pub epoc_last: Decimal,
    /// End time of the candle (seconds since epoch)
    pub epoc_end: Decimal,
    /// Open price of the candle
    pub open: Decimal,
    /// High price of the candle
    pub high: Decimal,
    /// Low price of the candle
    pub low: Decimal,
    /// Close price of the candle
    pub close: Decimal,
    /// Volume-weighted average price of the candle
    pub vwap: Decimal,
    /// Volume of the candle
    pub volume: Decimal,
}

/// Possible subscription types in Kraken WS api (v1)
/// Only supported types are listed here
#[derive(Debug, Display, Clone, Ord, PartialOrd, Eq, PartialEq)]
#[non_exhaustive]
pub enum SubscriptionType {
    /// book
    Book,
    /// openOrders
    OpenOrders,
    /// trade
    Trade,
    /// ohlc
    Ohlc,
}

impl FromStr for SubscriptionType {
    type Err = &'static str;
    fn from_str(src: &str) -> Result<SubscriptionType, Self::Err> {
        match src {
            "book" => Ok(SubscriptionType::Book),
            "openOrders" => Ok(SubscriptionType::OpenOrders),
            "trade" => Ok(SubscriptionType::Trade),
            "ohlc" => Ok(SubscriptionType::Ohlc),
            _ => Err("unknown subscription type"),
        }
    }
}
