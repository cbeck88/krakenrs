use displaydoc::Display;
use rust_decimal::Decimal;
use serde_json::Value;
use std::{collections::BTreeMap, str::FromStr};

/// The state of the book for some asset pair
#[derive(Default, Clone, Eq, PartialEq)]
pub struct BookData {
    /// The current asks, sorted by price
    pub ask: BTreeMap<Decimal, BookEntry>,
    /// The current bids, sorted by price
    pub bid: BTreeMap<Decimal, BookEntry>,
    /// Indicates that the book data is invalid
    pub checksum_failed: bool,
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
        for (_, ask) in self.ask.iter() {
            ask.crc32(&mut hasher);
        }
        // bids must be sorted high to low
        for (_, bid) in self.bid.iter().rev() {
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
    fn update_internal(
        side: &mut BTreeMap<Decimal, BookEntry>,
        data: &Value,
    ) -> Result<(), &'static str> {
        let outer_array = data.as_array().ok_or("update was not a json array")?;
        for data in outer_array.iter() {
            let data = data
                .as_array()
                .ok_or("update did not contain a json array")?;
            let price_level_str = data[0]
                .as_str()
                .ok_or("price level was not a json string")?;
            let volume_str = data[1].as_str().ok_or("volume was not a json string")?;
            let timestamp_str = data[2].as_str().ok_or("timestamp was not a json string")?;

            let price_level =
                Decimal::from_str(price_level_str).map_err(|_| "could not parse price level")?;
            let volume = Decimal::from_str(volume_str).map_err(|_| "could not parse volume")?;
            let timestamp =
                Decimal::from_str(timestamp_str).map_err(|_| "could not parse timestamp")?;

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

/// Possible subscription types in Kraken WS api
/// Only supported types are listed here
#[derive(Debug, Display, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum SubscriptionType {
    /// book
    Book,
}

impl FromStr for SubscriptionType {
    type Err = &'static str;
    fn from_str(src: &str) -> core::result::Result<SubscriptionType, Self::Err> {
        match src {
            "book" => Ok(SubscriptionType::Book),
            _ => Err("unknown subscription type"),
        }
    }
}

/// Possible subscription status types in Kraken WS api
#[derive(Debug, Display, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum SubscriptionStatus {
    /// subscribed
    Subscribed,
    /// unsubscribed
    Unsubscribed,
    /// error
    Error,
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
