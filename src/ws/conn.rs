use displaydoc::Display;
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    net::TcpStream,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tungstenite::{connect, stream::MaybeTlsStream, Message, WebSocket};

type WsClient = WebSocket<MaybeTlsStream<TcpStream>>;

pub use tungstenite::Error;

#[derive(Default, Clone)]
pub struct KrakenWsConfig {
    /// Order books to subscribe to
    pub subscribe_book: Vec<String>,
}

/// A sink where the ws worker can put updates for subscribed data
#[derive(Default)]
pub struct ApiResults {
    /// Map Asset Pair -> Book data
    pub book: HashMap<String, Mutex<BookData>>,
    /// Indicates that the stream is closed permanently
    pub stream_closed: AtomicBool,
}

/// The state of the book for some asset pair
#[derive(Default, Clone)]
pub struct BookData {
    /// The current asks, sorted by price from low to high
    pub ask: Vec<BookEntry>,
    /// The current bids, sorted by price from high to low
    pub bid: Vec<BookEntry>,
}

impl BookData {
    pub fn checksum(&self) -> u32 {
        let mut hasher = crc32fast::Hasher::new();
        for ask in &self.ask {
            ask.crc32(&mut hasher);
        }
        for bid in &self.bid {
            bid.crc32(&mut hasher);
        }
        hasher.finalize()
    }
}

/// An entry in an order book
#[derive(Default, Clone)]
pub struct BookEntry {
    /// The price of this book entry (Decimal)
    pub price: String,
    /// The volume of this book entry (Decimal)
    pub volume: String,
    /// The timestamp of this of this book entry (Decimal) (seconds since epoch)
    pub timestamp: String,
}

impl BookEntry {
    fn crc32(&self, hasher: &mut crc32fast::Hasher) {
        hasher.update(Self::format_str_for_hash(&self.price).as_bytes());
        hasher.update(Self::format_str_for_hash(&self.volume).as_bytes());
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

/// A Kraken websockets api client
pub struct KrakenWsClient {
    // config we were created with
    config: KrakenWsConfig,
    // websocket
    socket: WsClient,
    // output
    output: Arc<ApiResults>,
    // Channel-name -> AssetPair -> SubscriptionStatus
    book_subscriptions: HashMap<String, HashMap<String, SubscriptionStatus>>,
}

impl KrakenWsClient {
    /// Create a new Kraken Websockets Client from config
    /// (Only public api at time of writing)
    pub fn new(config: KrakenWsConfig) -> Result<(Self, Arc<ApiResults>), Error> {
        let (socket, _http_response) = tungstenite::connect("wss://ws.kraken.com")?;

        let mut api_results = ApiResults::default();
        for pair in config.subscribe_book.iter() {
            api_results
                .book
                .insert(pair.to_string(), Mutex::new(Default::default()));
        }

        let output = Arc::new(api_results);
        let mut result = Self {
            config: config.clone(),
            socket,
            output: output.clone(),
            book_subscriptions: Default::default(),
        };

        for pair in config.subscribe_book.iter() {
            result.subscribe_book(pair.to_string())?;
        }

        Ok((result, output))
    }

    /// Subscribe to a book stream
    fn subscribe_book(&mut self, pair: String) -> Result<(), Error> {
        unimplemented!()
    }

    /// Poll websocket for updates and apply them
    /// Blocks until at least one update occurs (TODO)
    pub fn update(&mut self) -> Result<(), Error> {
        match self.socket.read_message() {
            Ok(Message::Text(text)) => {
                self.handle_kraken_text(text);
                Ok(())
            }
            Ok(Message::Binary(_)) => {
                eprintln!("Warn: Unexpected binary message from Kraken");
                Ok(())
            }
            Ok(Message::Ping(_)) => Ok(()),
            Ok(Message::Pong(_)) => Ok(()),
            Ok(Message::Close(_)) => {
                // Per documentation: https://github.com/snapview/tungstenite-rs/blob/68541e409543f5bff9d2d4913b7521c58ae00c04/src/protocol/mod.rs#L154
                // Keep trying to write pending until ConnectionClosed is returned
                loop {
                    match self.socket.write_pending() {
                        Err(Error::ConnectionClosed) => {
                            self.output.stream_closed.store(true, Ordering::SeqCst);
                            return Err(Error::ConnectionClosed);
                        }
                        _ => {}
                    }
                }
            }
            Err(Error::ConnectionClosed) => {
                self.output.stream_closed.store(true, Ordering::SeqCst);
                Err(Error::ConnectionClosed)
            }
            Err(err) => Err(err),
        }
    }

    fn handle_kraken_text(&mut self, text: String) {
        match Value::from_str(&text) {
            Ok(Value::Object(map)) => {
                if let Some(event) = map.get("event") {
                    if event == "subscriptionStatus" {
                        if let Err(err) = self.handle_subscription_status(map) {
                            eprintln!("Error: handling subscription status: {}\n{}", err, text)
                        }
                    } else if event == "systemStatus" {
                        self.handle_system_status(map);
                    } else if event == "pong" || event == "heartbeat" {
                        // nothing to do
                    } else {
                        eprintln!("Error: Unknown event from kraken: {}\n{}", event, text);
                    }
                } else {
                    eprintln!(
                        "Error: Missing event string in payload from Kraken: {}",
                        text
                    );
                }
            }
            Ok(Value::Array(array)) => {
                if let Err(err) = self.handle_array(array) {
                    eprintln!("Error: handling array payload: {}\n{}", err, text);
                }
            }
            Ok(val) => {
                eprintln!("Error: Unexpected json value from Kraken: {:?}", val);
            }
            Err(err) => {
                eprintln!(
                    "Error: Could not deserialize json from Kraken: {}\n{}",
                    err, text
                );
            }
        }
    }

    fn handle_subscription_status(
        &mut self,
        map: serde_json::Map<String, Value>,
    ) -> Result<(), &'static str> {
        let channel_name =
            if let Value::String(name) = map.get("channelName").ok_or("Missing channelName")? {
                name.clone()
            } else {
                return Err("channelName was not a string");
            };
        let pair = if let Value::String(name) = map.get("pair").ok_or("Missing pair")? {
            name.clone()
        } else {
            return Err("pair was not a string");
        };
        if let Some(err) = map.get("errorMessage") {
            if let Value::String(err_msg) = err {
                eprintln!(
                    "Subscription ({}, {}) error: {}",
                    channel_name, pair, err_msg
                );
            } else {
                return Err("errorMessage is not a string");
            }
            return Ok(());
        }
        let status = if let Value::String(stat) = map.get("status").ok_or("Missing status")? {
            SubscriptionStatus::from_str(stat)?
        } else {
            return Err("status is not a string");
        };
        let subscription =
            if let Value::Object(obj) = map.get("subscription").ok_or("Missing subscription")? {
                if let Value::String(name) = obj.get("name").ok_or("Missing subscription.name")? {
                    SubscriptionType::from_str(name)?
                } else {
                    return Err("subscription.name is not a string");
                }
            } else {
                return Err("subscription is not an object");
            };
        match subscription {
            SubscriptionType::Book => match status {
                SubscriptionStatus::Subscribed => {
                    let stat = self
                        .book_subscriptions
                        .entry(channel_name.clone())
                        .or_default()
                        .entry(pair.clone())
                        .or_default();
                    if *stat != status {
                        eprintln!("Subscribed to {} {}", channel_name, pair);
                        *stat = status;
                    } else {
                        eprintln!("Unexpected repeated subscription message: {:?}", map);
                    }
                }
                SubscriptionStatus::Unsubscribed => {
                    let stat = self
                        .book_subscriptions
                        .entry(channel_name.clone())
                        .or_default()
                        .entry(pair.clone())
                        .or_default();
                    if *stat != status {
                        eprintln!("Unsubscribed from {} {}", channel_name, pair);
                        *stat = status;
                    } else {
                        eprintln!("Unexpected repeated unsubscription message: {:?}", map);
                    }
                }
                SubscriptionStatus::Error => {}
            },
        }
        Ok(())
    }

    fn handle_array(&mut self, array: Vec<Value>) -> Result<(), &'static str> {
        if array.len() < 2 {
            return Err("array too small");
        }
        if let Value::String(channel_name) = array
            .get(array.len() - 2)
            .ok_or("channel name index invalid")?
        {
            if let Some(book_channel) = self.book_subscriptions.get(channel_name) {
                // This looks like a book message. The last item should be the asset pair
                if let Value::String(pair) = array.get(array.len() - 1).ok_or("index invalid")? {
                    // Check if this matches a book subscription
                    if let Some(stat) = book_channel.get(pair) {
                        if *stat != SubscriptionStatus::Subscribed {
                            return Err("unexpected book message, not subscribed");
                        }

                        // This is an expected book message, lets scan across the objects
                        // Compare this logic with go code: https://github.com/jurijbajzelj/kraken_ws_orderbook/blob/16646c428b458474a2e3aa5d7025dd9e4d675598/ws/kraken.go#L128
                        // or python code: https://support.kraken.com/hc/en-us/articles/360027677512-Example-order-book-code-Python-
                        let len = array.len();
                        for val in &array[1..len - 2] {
                            if let Value::Object(obj) = val {
                                if let Some(ask_snapshot_val) = obj.get("as") {
                                    // Looks like a snapshot, had "as"
                                    if let Value::Array(as_array) = ask_snapshot_val {}
                                }
                            }
                        }
                        unimplemented!()
                    } else {
                        return Err("unexpected book message, never subscribed");
                    }
                } else {
                    return Err("book message did not have asset pair as last item");
                }
            } else {
                return Err("unexpected channel name");
            }
        } else {
            return Err("channel name index not a string");
        }
    }

    fn handle_system_status(
        &mut self,
        map: serde_json::Map<String, Value>,
    ) -> Result<(), &'static str> {
        unimplemented!()
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
    fn from_str(src: &str) -> core::result::Result<SubscriptionStatus, Self::Err> {
        match src {
            "subscribed" => Ok(SubscriptionStatus::Subscribed),
            "unsubscribed" => Ok(SubscriptionStatus::Unsubscribed),
            "error" => Ok(SubscriptionStatus::Error),
            _ => Err("unknown subscription status"),
        }
    }
}
