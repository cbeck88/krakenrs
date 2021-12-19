use super::{
    messages::{OrderInfo, OrderStatus, SubscriptionStatus, SystemStatus},
    types::{BookData, SubscriptionType},
};
use serde_json::{json, Value};
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    net::TcpStream,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tungstenite::{stream::MaybeTlsStream, Message, WebSocket};

type WsClient = WebSocket<MaybeTlsStream<TcpStream>>;

pub use tungstenite::Error;

/// Configuration for the websocket connection and feeds to subscribe to
#[derive(Clone, Debug)]
pub struct KrakenWsConfig {
    /// Order books to subscribe to
    pub subscribe_book: Vec<String>,
    /// Depth of order book subscriptions (how many ask/bid entries)
    pub book_depth: usize,
    /// Optional configuration for private feeds
    pub private: Option<KrakenPrivateWsConfig>,
}

impl Default for KrakenWsConfig {
    fn default() -> Self {
        Self {
            subscribe_book: Default::default(),
            book_depth: 10,
            private: None,
        }
    }
}

/// Configuration for private websockets feeds
#[derive(Clone, Debug)]
pub struct KrakenPrivateWsConfig {
    /// Authentication token (get from REST API)
    pub token: String,
    /// If true, subscribe to own orders feed for this account
    pub subscribe_open_orders: bool,
}

/// A sink where the ws worker can put updates for subscribed data
#[derive(Default)]
pub struct WsAPIResults {
    /// Current system status
    pub system_status: Mutex<Option<SystemStatus>>,
    /// Map Asset Pair -> Book data
    pub book: HashMap<String, Mutex<BookData>>,
    /// Map order id -> open orders
    pub open_orders: Mutex<HashMap<String, OrderInfo>>,
    /// Indicates that the stream is closed permanently
    pub stream_closed: AtomicBool,
}

/// A Kraken websockets api client
pub struct KrakenWsClient {
    // config we were created with
    config: KrakenWsConfig,
    // websocket
    socket: WsClient,
    // output
    output: Arc<WsAPIResults>,
    // Tracks subscription statuses of book subscriptions for multiple pairs
    // Channel-name -> AssetPair -> SubscriptionStatus
    book_subscriptions: HashMap<String, HashMap<String, SubscriptionStatus>>,
    // Tracks subscription status of the openOrders subscription (and channel)
    // This is Some if we are subscribed, and contains the current sequence number
    // It is None if we are unsubscribed or there was an error
    open_orders_subscription: Option<u64>,
    // Last subscribe attempt
    last_subscribe_attempt: Instant,
}

impl KrakenWsClient {
    /// Create a new Kraken Websockets Client from config
    /// (Only public api at time of writing)
    pub fn new(config: KrakenWsConfig) -> Result<(Self, Arc<WsAPIResults>), Error> {
        let (socket, _http_response) = tungstenite::connect(if config.private.is_some() {
            "wss://ws-auth.kraken.com"
        } else {
            "wss://ws.kraken.com"
        })?;

        // Pre-populate API Results with book data we plan to subscribe to
        let mut api_results = WsAPIResults::default();
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
            open_orders_subscription: None,
            last_subscribe_attempt: Instant::now(),
        };

        for pair in config.subscribe_book.iter() {
            result.subscribe_book(pair.to_string())?;
        }

        if config.private.is_some() {
            result.subscribe_open_orders()?;
        }

        Ok((result, output))
    }

    /// Subscribe to a book stream
    fn subscribe_book(&mut self, pair: String) -> Result<(), Error> {
        let payload = json!({
            "event": "subscribe",
            "pair": [pair],
            "subscription": {
                "name": "book",
                "depth": self.config.book_depth,
            },
        });
        self.socket
            .write_message(Message::Text(payload.to_string()))
    }

    /// Subscribe to the openOrders strream
    fn subscribe_open_orders(&mut self) -> Result<(), Error> {
        let private_config = self
            .config
            .private
            .as_ref()
            .expect("Can't subscribe to open orders without a token, this is a logic error");
        let payload = json!({
            "event": "subscribe",
            "subscription": {
                "name": "openOrders",
                "token": private_config.token.clone(),
            },
        });
        self.socket
            .write_message(Message::Text(payload.to_string()))
    }

    /// Poll websocket for updates and apply them
    /// Blocks until at least one update occurs
    ///
    /// Returns Error::ConnectionClosed if the stream has been closed
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
                self.finish_close();
                return Err(Error::ConnectionClosed);
            }
            Err(Error::ConnectionClosed) => {
                self.output.stream_closed.store(true, Ordering::SeqCst);
                Err(Error::ConnectionClosed)
            }
            Err(err) => Err(err),
        }
    }

    /// Resubscribe to any subscription that kraken unsubscribed us from (due to system outage)
    pub fn maybe_resubscribe(&mut self) -> Result<(), &'static str> {
        // Don't do this more than once every 10 seconds
        let now = Instant::now();
        if self.last_subscribe_attempt + Duration::from_secs(10) > now {
            return Ok(());
        }
        self.last_subscribe_attempt = now;

        let mut active_book_subscriptions = HashSet::<String>::default();
        for (channel_name, data) in &self.book_subscriptions {
            if channel_name.contains("book") {
                for (pair, status) in data {
                    if *status == SubscriptionStatus::Subscribed {
                        active_book_subscriptions.insert(pair.to_string());
                    }
                }
            }
        }

        for asset_pair in self.config.subscribe_book.clone() {
            if !active_book_subscriptions.contains(&asset_pair) {
                eprintln!("Info: Resubscribing to book '{}'", asset_pair);
                if let Err(err) = self.subscribe_book(asset_pair.to_string()) {
                    eprintln!(
                        "Error: Could not subscribe to book '{}': {}",
                        asset_pair, err
                    );
                }
            }
        }

        if let Some(private_config) = self.config.private.as_ref() {
            if private_config.subscribe_open_orders && self.open_orders_subscription.is_none() {
                eprintln!("Info: Resubscribing to openOrders");
                if let Err(err) = self.subscribe_open_orders() {
                    eprintln!("Error: Could not subscribe to openOrders: {}", err);
                }
            }
        }
        Ok(())
    }

    /// Close the socket gracefully
    pub fn close(&mut self) -> Result<(), Error> {
        self.socket.close(None)?;
        // Note: We don't finish_close here because Kraken servers don't seem to send ConnectionClosed back
        Ok(())
    }

    fn finish_close(&mut self) {
        // Per documentation: https://github.com/snapview/tungstenite-rs/blob/68541e409543f5bff9d2d4913b7521c58ae00c04/src/protocol/mod.rs#L154
        // Keep trying to write pending until ConnectionClosed is returned
        loop {
            match self.socket.write_pending() {
                Err(Error::ConnectionClosed) => {
                    self.output.stream_closed.store(true, Ordering::SeqCst);
                    return;
                }
                Err(err) => {
                    eprintln!("When closing socket: {}", err);
                }
                Ok(_) => {}
            }
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
                        if let Err(err) = self.handle_system_status(map) {
                            eprintln!("Error: handling system status: {}\n{}", err, text)
                        }
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
        let status = SubscriptionStatus::from_str(
            map.get("status")
                .ok_or("Missing status")?
                .as_str()
                .ok_or("status is not a string")?,
        )?;
        match status {
            SubscriptionStatus::Error => {
                let err_msg = map
                    .get("errorMessage")
                    .ok_or("missing errorMessage")?
                    .as_str()
                    .ok_or("errorMessage is not a string")?;
                eprintln!("subscription error: {}", err_msg);
                return Err("subscription error");
            }
            SubscriptionStatus::Subscribed | SubscriptionStatus::Unsubscribed => {
                let subscription = SubscriptionType::from_str(
                    map.get("subscription")
                        .ok_or("Missing subscription")?
                        .as_object()
                        .ok_or("subscription is not an object")?
                        .get("name")
                        .ok_or("Missing subscription.name")?
                        .as_str()
                        .ok_or("subscription.name is not a string")?,
                )?;

                let channel_name = map
                    .get("channelName")
                    .ok_or("Missing channelName")?
                    .as_str()
                    .ok_or("channelName was not a string")?
                    .clone();

                match subscription {
                    SubscriptionType::Book => {
                        // Book subscriptions refer to a pair
                        let pair = map
                            .get("pair")
                            .ok_or("Missing pair")?
                            .as_str()
                            .ok_or("pair was not a string")?
                            .clone();

                        let stat = self
                            .book_subscriptions
                            .entry(channel_name.to_string())
                            .or_default()
                            .entry(pair.to_string())
                            .or_default();
                        if *stat != status {
                            eprintln!("{} @ {} book: {}", status, pair, channel_name);
                            *stat = status;
                        } else {
                            eprintln!("Unexpected repeated {} message: {:?}", status, map);
                        }
                    }
                    SubscriptionType::OpenOrders => {
                        if status.is_subscribed() {
                            if self.open_orders_subscription.is_none() {
                                eprintln!("Subscribed to {}", channel_name);
                                // Initialize to zero so that first sequence number will be one larger
                                self.open_orders_subscription = Some(0);
                            } else {
                                eprintln!("Unexpected repeated {} message: {:?}", status, map);
                            }
                        } else {
                            if self.open_orders_subscription.is_some() {
                                eprintln!("Unsubscribed from {}", channel_name);
                                self.open_orders_subscription = None;
                            } else {
                                eprintln!("Unexpected repeated {} message: {:?}", status, map);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_array(&mut self, array: Vec<Value>) -> Result<(), &'static str> {
        if array.len() < 2 {
            return Err("array too small");
        }
        let channel_name = array
            .get(array.len() - 2)
            .ok_or("channel name index invalid")?
            .as_str()
            .ok_or("channel name was not a string")?;

        if channel_name == "openOrders" {
            // This is an openOrders message. Check the sequence number
            {
                let sequence_number = array
                    .get(array.len() - 1)
                    .ok_or("index invalid")?
                    .as_object()
                    .ok_or("expected an object for sequence number")?
                    .get("sequence")
                    .ok_or("missing sequence number")?
                    .as_u64()
                    .ok_or("sequence number was not an integer")?;
                let our_seq_number = self
                    .open_orders_subscription
                    .as_mut()
                    .ok_or("unexpected openOrders message")?;
                if *our_seq_number + 1 != sequence_number {
                    // TODO: Unsubscribe from openOrders and resubscribe later?
                    return Err("openOrders sequence number mismatch");
                }
                *our_seq_number += 1;
            }
            // Apply the updates
            let mut open_orders = self.output.open_orders.lock().expect("mutex poisoned");
            let updates = array
                .get(0)
                .ok_or("index invalid")?
                .as_array()
                .ok_or("updates were not an array")?;
            for update in updates {
                for (order_id, val) in update
                    .as_object()
                    .ok_or("expected update to be an object")?
                {
                    match open_orders.entry(order_id.to_string()) {
                        Entry::Occupied(mut entry) => {
                            // This is likely a status update, lets see what to do
                            let status = val
                                .as_object()
                                .ok_or("order update was not an object")?
                                .get("status")
                                .ok_or("order update missing status")?;
                            let status: OrderStatus = serde_json::from_value(status.clone())
                                .map_err(|err| {
                                    eprintln!("Could not parse order status: {}", err);
                                    "OrderStatus deserialization error"
                                })?;
                            match status {
                                OrderStatus::Pending | OrderStatus::Open => {
                                    entry.get_mut().status = status;
                                }
                                OrderStatus::Closed
                                | OrderStatus::Expired
                                | OrderStatus::Canceled => {
                                    entry.remove();
                                }
                            }
                        }
                        Entry::Vacant(entry) => {
                            // Parse the data as an OrderInfo object and add the new order id
                            let order_info : OrderInfo = serde_json::from_value(val.clone()).map_err(|err| {
                                eprintln!("Could not parse open order data as an OrderInfo object: {}", err);
                                "OrderInfo deserialization error"
                            })?;
                            entry.insert(order_info);
                        }
                    }
                }
            }
            return Ok(());
        } else if let Some(book_channel) = self.book_subscriptions.get(channel_name) {
            // This looks like a book message. The last item should be the asset pair
            let pair = array
                .get(array.len() - 1)
                .ok_or("index invalid")?
                .as_str()
                .ok_or("book message did not have asset pair string as last item")?;

            // Check if this matches a book subscription
            let stat = book_channel
                .get(pair)
                .ok_or("unexpected book message, never subscribed to asset pair")?;
            if *stat != SubscriptionStatus::Subscribed {
                return Err("unexpected book message, not subscribed");
            }

            // Lock the book data to perform the update
            let mut book = self
                .output
                .book
                .get(pair)
                .ok_or("missing asset pair in api results")?
                .lock()
                .expect("mutex poisoned");

            // This is an expected book message, lets figure out if it is a snapshot
            // Compare this logic with go code: https://github.com/jurijbajzelj/kraken_ws_orderbook/blob/16646c428b458474a2e3aa5d7025dd9e4d675598/ws/kraken.go#L128
            // or python code: https://support.kraken.com/hc/en-us/articles/360027677512-Example-order-book-code-Python-

            let first_obj = array[1]
                .as_object()
                .ok_or("expected an object with ask / bid updates")?;
            if first_obj.contains_key("as") {
                // Looks like a snapshot
                book.clear();
                {
                    let ask_snapshot_val = first_obj.get("as").ok_or("expected an ask snapshot")?;
                    book.update_asks(ask_snapshot_val, self.config.book_depth)?;
                }
                {
                    let bid_snapshot_val = first_obj.get("bs").ok_or("expected a bid snapshot")?;
                    book.update_bids(bid_snapshot_val, self.config.book_depth)?;
                }
            } else if first_obj.contains_key("a") || first_obj.contains_key("b") {
                // Looks like an incremental update
                // lets scan across the objects in the array, skipping first and last two
                drop(first_obj);
                let len = array.len();
                for val in &array[1..len - 2] {
                    let obj = val.as_object().ok_or("expected an update object")?;
                    if let Some(ask_val) = obj.get("a") {
                        book.update_asks(ask_val, self.config.book_depth)?;
                    }
                    if let Some(bid_val) = obj.get("b") {
                        book.update_bids(bid_val, self.config.book_depth)?;
                    }
                    // If we got a checksum, lets check it
                    if let Some(check_val) = obj.get("c") {
                        let expected_checksum = u32::from_str(
                            check_val
                                .as_str()
                                .ok_or("checksum value was not a string")?,
                        )
                        .map_err(|_| "checksum value could not parse as u32")?;
                        let checksum = book.checksum();
                        if checksum != expected_checksum {
                            eprintln!("Error: checksum mismatch, book is out of sync.");
                            book.checksum_failed = true;
                            // TODO: Unsubscribe from this book? maybe_resubscribe will happen later
                            return Err("checksum mismatch");
                        }
                    }
                }
            } else {
                return Err("update had no usable data");
            }
            return Ok(());
        } else {
            return Err("unexpected channel name");
        }
    }

    fn handle_system_status(
        &mut self,
        map: serde_json::Map<String, Value>,
    ) -> Result<(), &'static str> {
        let status = SystemStatus::from_str(
            map.get("status")
                .ok_or("missing status field")?
                .as_str()
                .ok_or("status was not a string")?,
        )?;
        *self.output.system_status.lock().expect("mutex poisoned") = Some(status);
        Ok(())
    }
}

impl Drop for KrakenWsClient {
    fn drop(&mut self) {
        self.output.stream_closed.store(true, Ordering::SeqCst);
    }
}
