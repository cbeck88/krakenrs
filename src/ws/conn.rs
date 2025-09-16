use super::{
    config::KrakenWsConfig,
    messages::{AddOrderRequest, BsType, OrderInfo, OrderStatus, SubscriptionStatus, SystemStatus},
    types::{BookData, Candle, PublicTrade, SubscriptionType},
};
use futures::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use http::Uri;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    str::FromStr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{net::TcpStream, sync::oneshot};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;
type SinkType = SplitSink<WsClient, Message>;

pub use tokio_tungstenite::tungstenite::Error;

/// When we want to change whether or not we are subscribed to a feed, we wait
/// this long before we reissue the subscribe / unsubscribe request
const SUBSCRIPTION_CHANGE_BACKOFF: Duration = Duration::from_secs(5);

/// A sink where the ws worker can put updates for subscribed data
#[derive(Default)]
#[non_exhaustive]
pub struct WsAPIResults {
    /// Current system status
    pub system_status: Mutex<Option<SystemStatus>>,
    /// Map Asset Pair -> Book data
    pub book: HashMap<String, Mutex<BookData>>,
    /// Map Asset Pair -> Ohlc data
    pub ohlc: HashMap<String, Mutex<Vec<Candle>>>,
    /// Map Asset Pair -> Public trade data
    pub trades: HashMap<String, Mutex<Vec<PublicTrade>>>,
    /// Map order id -> open orders
    pub open_orders: Mutex<HashMap<String, OrderInfo>>,
    /// Indicates that the stream is closed right now, and data may be stale.
    pub stream_closed: AtomicBool,
}

/// A Kraken websockets api context.
/// Owns the websockets stream output, and all context related to the protocol
/// Does not drive the websocket on its own however, the caller needs to poll the
/// stream and then call "update", as well as giving it any requests to send to kraken.
pub struct KrakenWsClient {
    /// config we were created with
    config: KrakenWsConfig,
    /// websocket sink
    sink: SinkType,
    /// output
    output: Arc<WsAPIResults>,
    /// Track subscription statuses of different channels
    subscription_tracker: SubscriptionTracker,
    /// Result senders for add_order calls
    add_order_result_senders: HashMap<u64, oneshot::Sender<Result<String, String>>>,
    /// Result senders for cancel_order calls
    cancel_order_result_senders: HashMap<u64, oneshot::Sender<Result<(), String>>>,
    /// Result senders for cancel_all_orders calls
    cancel_all_orders_result_senders: HashMap<u64, oneshot::Sender<Result<u64, String>>>,
    /// Client req id ensures unique ids for different requests we make to kraken
    client_req_id: AtomicU64,
    /// The last time if any that we got a message from Kraken, including heartbeats
    last_msg_received: Option<Instant>,
    /// The last time, and req-id, for a ping that we sent to Kraken
    last_outstanding_ping: Option<(Instant, u64)>,
}

impl KrakenWsClient {
    /// Create a new Kraken Websockets Client
    ///
    /// Returns:
    /// * The websockets client object, which contains all websockets and Kraken protocol context
    /// * The stream portion of the websockets connection. This should be polled by
    ///   the caller and the result passed to "update". The client and stream should be
    ///   dropped if update yields an error.
    ///
    ///   Note: Use [crate::ws::KrakenWsAPI] if you want a batteries included version of this.
    ///   If you want control over exactly how that
    ///   polling is working then you should call `KrakenWsClient::new` and wire it
    ///   up as you like.
    /// * `Arc<WsApiResults>`. This may be shared with synchronous code and polled for updates.
    ///   Note: [crate::ws::KrakenWsAPI] also conceals this detail.
    pub async fn new(config: KrakenWsConfig) -> Result<(Self, SplitStream<WsClient>, Arc<WsAPIResults>), Error> {
        let url: Uri = if config.private.is_some() {
            "wss://ws-auth.kraken.com".parse().unwrap()
        } else {
            "wss://ws.kraken.com".parse().unwrap()
        };
        let (socket, _request) = tokio_tungstenite::connect_async(url).await?;
        let (sink, stream) = socket.split();

        // Pre-populate API Results with book data we plan to subscribe to
        let mut api_results = WsAPIResults::default();
        for pair in config.subscribe_book.iter() {
            api_results.book.insert(pair.to_owned(), Mutex::new(Default::default()));
        }
        for pair in config.subscribe_trades.iter() {
            api_results
                .trades
                .insert(pair.to_owned(), Mutex::new(Default::default()));
        }
        for pair in config.subscribe_ohlc.iter() {
            api_results.ohlc.insert(pair.to_owned(), Mutex::new(Default::default()));
        }

        let output = Arc::new(api_results);
        let mut result = Self {
            config: config.clone(),
            sink,
            output: output.clone(),
            subscription_tracker: Default::default(),
            add_order_result_senders: Default::default(),
            cancel_order_result_senders: Default::default(),
            cancel_all_orders_result_senders: Default::default(),
            client_req_id: Default::default(),
            last_msg_received: None,
            last_outstanding_ping: None,
        };

        for pair in config.subscribe_book.iter() {
            result.subscription_tracker.get_book(pair.to_owned()).last_request =
                Some((SubscriptionStatus::Subscribed, Instant::now()));
            result.subscribe_book(pair.to_string()).await?;
        }

        for pair in config.subscribe_trades.iter() {
            result.subscription_tracker.get_trade(pair.to_owned()).last_request =
                Some((SubscriptionStatus::Subscribed, Instant::now()));
            result.subscribe_trade(pair.to_string()).await?;
        }

        for pair in config.subscribe_ohlc.iter() {
            result.subscription_tracker.get_ohlc(pair.to_owned()).last_request =
                Some((SubscriptionStatus::Subscribed, Instant::now()));
            result.subscribe_ohlc(pair.to_string()).await?;
        }

        if config.private.is_some() {
            // TODO: In the future, check config.subscribe_open_orders, and only
            // subscribe to open_orders if desired by the user.
            //
            // However, right now this is the only thing you can subscribe to,
            // and kraken says they will close the private connection if you don't
            // subscribe to any private feed.
            result.subscription_tracker.get_open_orders().last_request =
                Some((SubscriptionStatus::Subscribed, Instant::now()));
            result.subscribe_open_orders().await?;
        }

        Ok((result, stream, output))
    }

    /// Apply a result (or error) from the websocket stream to the kraken protocol context.
    ///
    /// Returns Ok when the message was handled successfully
    /// Errors should be considered fatal, and will result in stream_closed being set
    /// for the consumer.
    pub fn update(&mut self, stream_result: Result<Message, Error>) -> Result<(), Error> {
        if stream_result.is_ok() {
            self.last_msg_received = Some(Instant::now());
        }
        match stream_result {
            Ok(Message::Text(text)) => {
                self.handle_kraken_text(text.as_str());
            }
            Ok(Message::Binary(_)) => {
                log::warn!("Unexpected binary message from Kraken");
            }
            Ok(Message::Ping(_)) => {}
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => return Err(Error::ConnectionClosed),
            Ok(Message::Frame(_)) => {
                log::error!("Per docs, this should be unreachable when reading");
            }
            Err(err) => {
                self.output.stream_closed.store(true, Ordering::SeqCst);
                return Err(err);
            }
        }
        Ok(())
    }

    /// Resubscribe to any subscription that kraken unsubscribed us from (due to system outage)
    ///
    /// Any errors are logged
    pub async fn check_subscriptions(&mut self) {
        // First look for active subscriptions with errors and try to unsubscribe
        for (asset_pair, sub) in self.subscription_tracker.book_subscriptions.iter_mut() {
            if sub.status.is_subscribed() && sub.needs_unsubscribe && !sub.tried_to_change_recently() {
                sub.last_request = Some((SubscriptionStatus::Unsubscribed, Instant::now()));
                if let Err(err) =
                    Self::unsubscribe_book(&mut self.sink, self.config.book_depth, asset_pair.clone()).await
                {
                    log::error!("Could not unsubscribe from book {}: {}", asset_pair.clone(), err);
                }
            }
        }

        for (asset_pair, sub) in self.subscription_tracker.ohlc_subscriptions.iter_mut() {
            if sub.status.is_subscribed() && sub.needs_unsubscribe && !sub.tried_to_change_recently() {
                sub.last_request = Some((SubscriptionStatus::Unsubscribed, Instant::now()));
                if let Err(err) =
                    Self::unsubscribe_ohlc(&mut self.sink, self.config.ohlc_interval, asset_pair.clone()).await
                {
                    log::error!("Could not unsubscribe from ohlc {}: {}", asset_pair.clone(), err);
                }
            }
        }

        for (asset_pair, sub) in self.subscription_tracker.trade_subscriptions.iter_mut() {
            if sub.status.is_subscribed() && sub.needs_unsubscribe && !sub.tried_to_change_recently() {
                sub.last_request = Some((SubscriptionStatus::Unsubscribed, Instant::now()));
                if let Err(err) = Self::unsubscribe_trade(&mut self.sink, asset_pair.clone()).await {
                    log::error!("Could not unsubscribe from trade {}: {}", asset_pair.clone(), err);
                }
            }
        }

        {
            let sub = self.subscription_tracker.get_open_orders();
            if sub.status.is_subscribed() && sub.needs_unsubscribe && !sub.tried_to_change_recently() {
                sub.last_request = Some((SubscriptionStatus::Unsubscribed, Instant::now()));
                if let Err(err) = self.unsubscribe_open_orders().await {
                    log::error!("Could not unsubscribe from open orders: {}", err);
                }
            }
        }

        // Now look for things we are not subscribed to that we should be.
        // Check all the requested subscriptions
        for asset_pair in self.config.subscribe_book.clone() {
            let sub = self.subscription_tracker.get_book(asset_pair.to_string());
            if !sub.status.is_subscribed() && !sub.tried_to_change_recently() {
                log::info!("Resubscribing to book '{}'", asset_pair);
                sub.last_request = Some((SubscriptionStatus::Subscribed, Instant::now()));
                if let Err(err) = self.subscribe_book(asset_pair.to_string()).await {
                    log::error!("Could not subscribe to book '{}': {}", asset_pair, err);
                }
            }
        }

        for asset_pair in self.config.subscribe_ohlc.clone() {
            let sub = self.subscription_tracker.get_ohlc(asset_pair.to_string());
            if !sub.status.is_subscribed() && !sub.tried_to_change_recently() {
                log::info!("Resubscribing to ohlc '{}'", asset_pair);
                sub.last_request = Some((SubscriptionStatus::Subscribed, Instant::now()));
                if let Err(err) = self.subscribe_ohlc(asset_pair.to_string()).await {
                    log::error!("Could not subscribe to ohlc '{}': {}", asset_pair, err);
                }
            }
        }

        for asset_pair in self.config.subscribe_trades.clone() {
            let sub = self.subscription_tracker.get_trade(asset_pair.to_string());
            if !sub.status.is_subscribed() && !sub.tried_to_change_recently() {
                log::info!("Resubscribing to trade '{}'", asset_pair);
                sub.last_request = Some((SubscriptionStatus::Subscribed, Instant::now()));
                if let Err(err) = self.subscribe_trade(asset_pair.to_string()).await {
                    log::error!("Could not subscribe to trade '{}': {}", asset_pair, err);
                }
            }
        }

        if let Some(private_config) = self.config.private.as_ref()
            && private_config.subscribe_open_orders
        {
            let sub = self.subscription_tracker.get_open_orders();
            if !sub.status.is_subscribed() && !sub.tried_to_change_recently() {
                log::info!("Resubscribing to openOrders");
                sub.last_request = Some((SubscriptionStatus::Subscribed, Instant::now()));
                if let Err(err) = self.subscribe_open_orders().await {
                    log::error!("Could not subscribe to openOrders: {}", err);
                }
            }
        }
    }

    /// Submit an order over the websocket
    ///
    /// The oneshot::Sender is sent Ok if the order is confirmed from Kraken,
    /// and the TxID of the order is returned. The error message from kraken is
    /// returned otherwise. The sender gets nothing if we fail to submit the order
    /// at all.
    pub async fn add_order(
        &mut self,
        mut order: AddOrderRequest,
        result_sender: oneshot::Sender<Result<String, String>>,
    ) -> Result<(), Error> {
        let token = if let Some(private_config) = self.config.private.as_ref() {
            private_config.token.clone()
        } else {
            log::error!("Tried to submit an order, but this is not an authenticated channel");
            // Drop the result_sender and do not signal an error to the websocket
            return Ok(());
        };

        let client_req_id = self.client_req_id.fetch_add(1, Ordering::SeqCst);
        order.event = "addOrder".into();
        order.reqid = Some(client_req_id);
        order.token = token;

        // This drops the result_sender if serialization or sending fails
        match serde_json::to_string(&order) {
            Err(err) => {
                log::error!("Could not serialize order: {}", err);
                return Ok(());
            }
            Ok(text) => {
                // We have to store the result_sender before awaiting
                self.add_order_result_senders.insert(client_req_id, result_sender);
                self.sink.send(Message::Text(text.into())).await.inspect_err(|_err| {
                    self.add_order_result_senders.remove(&client_req_id);
                })?;
            }
        }
        Ok(())
    }

    /// Submit a request to cancel an order over the websocket
    ///
    /// TxID may be a string used to identify an order, or a user-ref-id
    ///
    /// The oneshot::Sender is sent Ok if the cancel order is successful,
    /// and the error message from kraken otherwise. The sender gets nothing
    /// if we fail to submit the request at all.
    pub async fn cancel_order(
        &mut self,
        txid: String,
        result_sender: oneshot::Sender<Result<(), String>>,
    ) -> Result<(), Error> {
        let token = if let Some(private_config) = self.config.private.as_ref() {
            private_config.token.clone()
        } else {
            log::error!("Tried to submit an order, but this is not an authenticated channel");
            // Drop the result_sender and do not signal an error to the websocket
            return Ok(());
        };

        let client_req_id = self.client_req_id.fetch_add(1, Ordering::SeqCst);

        let payload = json! ({
            "event": "cancelOrder",
            "token": token,
            "txid": [txid],
            "reqid": client_req_id,
        });

        // We have to store the result_sender before awaiting
        self.cancel_order_result_senders.insert(client_req_id, result_sender);

        // This drops the result_sender if sending fails
        self.sink
            .send(Message::Text(payload.to_string().into()))
            .await
            .inspect_err(|_err| {
                self.cancel_order_result_senders.remove(&client_req_id);
            })?;

        Ok(())
    }

    /// Submit a request to cancel all orders over the websocket
    ///
    /// The oneshot::Sender is sent Ok if the cancel order is successful, with
    /// the number of orders canceled. The sender gets the error message from
    /// kraken otherwise. The sender gets nothing
    /// if we fail to submit the request at all.
    pub async fn cancel_all_orders(
        &mut self,
        result_sender: oneshot::Sender<Result<u64, String>>,
    ) -> Result<(), Error> {
        let token = if let Some(private_config) = self.config.private.as_ref() {
            private_config.token.clone()
        } else {
            log::error!("Tried to submit an order, but this is not an authenticated channel");
            // Drop the result_sender and do not signal an error to the websocket
            return Ok(());
        };

        let client_req_id = self.client_req_id.fetch_add(1, Ordering::SeqCst);

        let payload = json! ({
            "event": "cancelAll",
            "token": token,
            "reqid": client_req_id,
        });

        // We have to store the result_sender before awaiting
        self.cancel_all_orders_result_senders
            .insert(client_req_id, result_sender);

        // This drops the result_sender if sending fails
        self.sink
            .send(Message::Text(payload.to_string().into()))
            .await
            .inspect_err(|_err| {
                self.cancel_all_orders_result_senders.remove(&client_req_id);
            })?;

        Ok(())
    }

    /// Send a ping to the kraken server. This is an application-level ping
    /// and not a websockets ping.
    ///
    /// Note: It is a logic error to call this while
    /// `get_last_outstanding_ping_time` returns Some.
    pub async fn ping(&mut self) -> Result<(), Error> {
        let req_id = self.client_req_id.fetch_add(1, Ordering::SeqCst);

        let payload = json!({
            "event": "ping",
            "reqid": req_id,
        });

        self.sink.send(Message::Text(payload.to_string().into())).await?;

        self.last_outstanding_ping = Some((Instant::now(), req_id));
        Ok(())
    }

    /// Get the time of the last ping that was sent (if any).
    /// Returns none if that ping was answered with pong by kraken.
    pub fn get_last_outstanding_ping_time(&self) -> Option<Instant> {
        self.last_outstanding_ping.map(|x| x.0)
    }

    /// Get the time of the last message we received from Kraken (if any).
    pub fn get_last_message_time(&self) -> Option<Instant> {
        self.last_msg_received
    }

    /// Close the socket gracefully
    pub async fn close(&mut self) -> Result<(), Error> {
        self.output.stream_closed.store(true, Ordering::SeqCst);
        self.sink.close().await
    }

    /// Subscribe to a book stream
    async fn subscribe_book(&mut self, pair: String) -> Result<(), Error> {
        let payload = json!({
            "event": "subscribe",
            "pair": [pair],
            "subscription": {
                "name": "book",
                "depth": self.config.book_depth,
            },
        });
        self.sink.send(Message::Text(payload.to_string().into())).await
    }

    /// Unsubscribe from a book stream
    ///
    /// Note: We made this not take self, to resolve a borrow checker issue
    async fn unsubscribe_book(sink: &mut SinkType, book_depth: usize, pair: String) -> Result<(), Error> {
        let payload = json!({
            "event": "unsubscribe",
            "pair": [pair],
            "subscription": {
                "name": "book",
                "depth": book_depth,
            },
        });
        sink.send(Message::Text(payload.to_string().into())).await
    }

    /// Subscribe to a trade stream
    async fn subscribe_trade(&mut self, pair: String) -> Result<(), Error> {
        let payload = json!({
            "event": "subscribe",
            "pair": [pair],
            "subscription": {
                "name": "trade",
            },
        });
        self.sink.send(Message::Text(payload.to_string().into())).await
    }

    /// Unsubscribe from a trade stream
    ///
    /// Note: We made this not take self, to resolve a borrow checker issue
    async fn unsubscribe_trade(sink: &mut SinkType, pair: String) -> Result<(), Error> {
        let payload = json!({
            "event": "unsubscribe",
            "pair": [pair],
            "subscription": {
                "name": "trade",
            },
        });
        sink.send(Message::Text(payload.to_string().into())).await
    }

    /// Subscribe to an ohlc stream
    async fn subscribe_ohlc(&mut self, pair: String) -> Result<(), Error> {
        let payload = json!({
            "event": "subscribe",
            "pair": [pair],
            "subscription": {
                "name": "ohlc",
                "interval": self.config.ohlc_interval,
            },
        });
        self.sink.send(Message::Text(payload.to_string().into())).await
    }

    /// Unsubscribe from an ohlc stream
    ///
    /// Note: We made this not take self, to resolve a borrow checker issue
    async fn unsubscribe_ohlc(sink: &mut SinkType, ohlc_interval: u16, pair: String) -> Result<(), Error> {
        let payload = json!({
            "event": "unsubscribe",
            "pair": [pair],
            "subscription": {
                "name": "ohlc",
                "interval": ohlc_interval,
            },
        });
        sink.send(Message::Text(payload.to_string().into())).await
    }

    /// Subscribe to the openOrders strream
    async fn subscribe_open_orders(&mut self) -> Result<(), Error> {
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
        self.sink.send(Message::Text(payload.to_string().into())).await
    }

    /// Unsubscribe from the openOrders strream
    async fn unsubscribe_open_orders(&mut self) -> Result<(), Error> {
        let private_config = self
            .config
            .private
            .as_ref()
            .expect("Can't subscribe to open orders without a token, this is a logic error");
        let payload = json!({
            "event": "unsubscribe",
            "subscription": {
                "name": "openOrders",
                "token": private_config.token.clone(),
            },
        });
        self.sink.send(Message::Text(payload.to_string().into())).await
    }

    fn handle_kraken_text(&mut self, text: &str) {
        match Value::from_str(text) {
            Ok(Value::Object(map)) => {
                if let Some(event) = map.get("event") {
                    if event == "subscriptionStatus" {
                        if let Err(err) = self.handle_subscription_status(map) {
                            log::error!("handling subscription status: {}\n{}", err, text)
                        }
                    } else if event == "systemStatus" {
                        if let Err(err) = self.handle_system_status(map) {
                            log::error!("handling system status: {}\n{}", err, text)
                        }
                    } else if event == "addOrderStatus" {
                        if let Err(err) = self.handle_add_order_status(map) {
                            log::error!("handling add order status: {}\n{}", err, text)
                        }
                    } else if event == "cancelOrderStatus" {
                        if let Err(err) = self.handle_cancel_order_status(map) {
                            log::error!("handling cancel order status: {}\n{}", err, text)
                        }
                    } else if event == "cancelAllStatus" {
                        if let Err(err) = self.handle_cancel_all_orders_status(map) {
                            log::error!("handling cancel all order status: {}\n{}", err, text)
                        }
                    } else if event == "pong" {
                        if let Err(err) = self.handle_pong(map) {
                            log::error!("handling pong: {}\n{}", err, text)
                        }
                    } else if event == "heartbeat" {
                        // nothing to do
                    } else {
                        log::error!("Unknown event from kraken: {}\n{}", event, text);
                    }
                } else {
                    log::error!("Missing event string in payload from Kraken: {}", text);
                }
            }
            Ok(Value::Array(array)) => {
                if let Err(err) = self.handle_array(array) {
                    log::error!("handling array payload: {}\n{}", err, text);
                }
            }
            Ok(val) => {
                log::error!("Unexpected json value from Kraken: {:?}", val);
            }
            Err(err) => {
                log::error!("Could not deserialize json from Kraken: {}\n{}", err, text);
            }
        }
    }

    fn handle_pong(&mut self, map: serde_json::Map<String, Value>) -> Result<(), &'static str> {
        if let Some(req_id_val) = map.get("reqid") {
            if let Some(req_id_num) = req_id_val.as_u64() {
                if let Some(last_ping) = self.last_outstanding_ping {
                    if last_ping.1 != req_id_num {
                        return Err("Received pong with unexpected reqid");
                    }
                } else {
                    return Err("Received pong without outstanding ping");
                }
            } else {
                return Err("Received pong with reqid that was not a number");
            }
        } else {
            return Err("Received a pong without reqid, not expected");
        }
        // If we got here, then the pong had the expected req id, so our ping was answered
        self.last_outstanding_ping = None;
        Ok(())
    }

    fn handle_subscription_status(&mut self, map: serde_json::Map<String, Value>) -> Result<(), &'static str> {
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
                log::error!("subscription error: {}", err_msg);
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
                    .ok_or("channelName was not a string")?;

                match subscription {
                    SubscriptionType::Book => {
                        // Book subscriptions refer to a pair
                        let pair = map
                            .get("pair")
                            .ok_or("Missing pair")?
                            .as_str()
                            .ok_or("pair was not a string")?;

                        self.subscription_tracker.add_book_channel(channel_name.to_string());
                        let sub = self.subscription_tracker.get_book(pair.to_string());
                        if sub.status != status {
                            log::info!("{status} @ {pair} book: {channel_name}");
                            *sub = SubscriptionState::new(status);
                        } else {
                            log::warn!("Unexpected repeated {} message: {:?}", status, map);
                        }
                    }
                    SubscriptionType::Ohlc => {
                        let pair = map
                            .get("pair")
                            .ok_or("Missing pair")?
                            .as_str()
                            .ok_or("pair was not a string")?;

                        self.subscription_tracker.add_ohlc_channel(channel_name.to_string());
                        let sub = self.subscription_tracker.get_ohlc(pair.to_string());
                        if sub.status != status {
                            log::info!("{status} @ {pair} ohlc: {channel_name}");
                            *sub = SubscriptionState::new(status);
                        } else {
                            log::warn!("Unexpected repeated {} message: {:?}", status, map);
                        }
                    }
                    SubscriptionType::Trade => {
                        // Trade subscriptions refer to a pair
                        let pair = map
                            .get("pair")
                            .ok_or("Missing pair")?
                            .as_str()
                            .ok_or("pair was not a string")?;

                        let sub = self.subscription_tracker.get_trade(pair.to_string());
                        if sub.status != status {
                            log::info!("{status} @ {pair} trade: {channel_name}");
                            *sub = SubscriptionState::new(status);
                        } else {
                            log::warn!("Unexpected repeated {} message: {:?}", status, map);
                        }
                    }
                    SubscriptionType::OpenOrders => {
                        let sub = self.subscription_tracker.get_open_orders();
                        if sub.status != status {
                            *sub = SubscriptionState::new(status);
                            if status.is_subscribed() {
                                log::info!("Subscribed to {}", channel_name);
                            } else {
                                log::info!("Unsubscribed from {}", channel_name);
                            }
                        } else {
                            log::warn!("Unexpected repeated {} message: {:?}", status, map);
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
                    .last()
                    .ok_or("index invalid")?
                    .as_object()
                    .ok_or("expected an object for sequence number")?
                    .get("sequence")
                    .ok_or("missing sequence number")?
                    .as_u64()
                    .ok_or("sequence number was not an integer")?;
                self.subscription_tracker
                    .get_open_orders()
                    .check_sequence_number(sequence_number)?;
            }
            // Apply the updates
            let mut open_orders = self.output.open_orders.lock().expect("mutex poisoned");
            let updates = array
                .first()
                .ok_or("index invalid")?
                .as_array()
                .ok_or("updates were not an array")?;
            for update in updates {
                for (order_id, val) in update.as_object().ok_or("expected update to be an object")? {
                    match open_orders.entry(order_id.to_string()) {
                        Entry::Occupied(mut entry) => {
                            // This is likely a status update, lets see what to do
                            let obj = val.as_object().ok_or("order update was not an object")?;

                            let status_val = match obj.get("status") {
                                Some(status_val) => status_val,
                                None => {
                                    log::trace!("Order update without status, this is likely a partial-fill message");
                                    continue;
                                }
                            };

                            let status: OrderStatus = serde_json::from_value(status_val.clone()).map_err(|err| {
                                log::error!("Could not parse order status: {}", err);
                                "OrderStatus deserialization error"
                            })?;
                            match status {
                                OrderStatus::Pending | OrderStatus::Open => {
                                    entry.get_mut().status = status;
                                }
                                OrderStatus::Closed | OrderStatus::Expired | OrderStatus::Canceled => {
                                    entry.remove();
                                }
                            }
                        }
                        Entry::Vacant(entry) => {
                            // Parse the data as an OrderInfo object and add the new order id
                            let order_info: OrderInfo = serde_json::from_value(val.clone()).map_err(|err| {
                                log::error!("Could not parse open order data as an OrderInfo object: {}", err);
                                "OrderInfo deserialization error"
                            })?;
                            entry.insert(order_info);
                        }
                    }
                }
            }
            Ok(())
        } else if channel_name == "trade" {
            // This looks like a trade message. The last item should be the asset pair
            let pair = array
                .last()
                .ok_or("index invalid")?
                .as_str()
                .ok_or("trade message did not have asset pair string as last item")?;

            // Check if this matches a trade subscription
            let sub = self.subscription_tracker.get_trade(pair.to_string());
            if !sub.status.is_subscribed() {
                return Err("unexpected trade message, not subscribed");
            }

            // Lock the trade data to perform the update
            let mut lk = self
                .output
                .trades
                .get(pair)
                .ok_or("unexpected asset pair update -- check asset pair name")?
                .lock()
                .expect("mutex poisoned");

            let trades_array = array[1].as_array().ok_or("expected array of trades")?;

            lk.reserve(trades_array.len());

            for ent in trades_array.iter() {
                let data = ent.as_array().ok_or("expected each trade to be an array")?;
                let price_str = data[0].as_str().ok_or("expected price to be a string")?;
                let volume_str = data[1].as_str().ok_or("volume was not a json string")?;
                let timestamp_str = if let Some(num) = data[2].as_number() {
                    num.as_str()
                } else if let Some(s) = data[2].as_str() {
                    s
                } else {
                    return Err("timestamp was not a json number or string");
                };
                let buy_sell_str = data[3].as_str().ok_or("buy-sell was expected to be a json string")?;

                let side = match buy_sell_str {
                    "b" => BsType::Buy,
                    "s" => BsType::Sell,
                    _ => {
                        return Err("buy-sell str was not 'b' or 's'");
                    }
                };
                let price = Decimal::from_str(price_str).map_err(|_| "could not parse price")?;
                let volume = Decimal::from_str(volume_str).map_err(|_| "could not parse volume")?;
                let timestamp = Decimal::from_str(timestamp_str).map_err(|_| "could not parse timestamp")?;

                lk.push(PublicTrade {
                    price,
                    volume,
                    timestamp,
                    side,
                });
            }

            Ok(())
        } else if self.subscription_tracker.is_book_channel(channel_name) {
            // This looks like a book message. The last item should be the asset pair
            let pair = array
                .last()
                .ok_or("index invalid")?
                .as_str()
                .ok_or("book message did not have asset pair string as last item")?;

            // Check if this matches a book subscription
            let sub = self.subscription_tracker.get_book(pair.to_string());
            if !sub.status.is_subscribed() {
                return Err("unexpected book message, not subscribed");
            }

            // Lock the book data to perform the update
            let mut book = self
                .output
                .book
                .get(pair)
                .ok_or("unexpected asset pair update -- check asset pair name")?
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
                        let expected_checksum =
                            u32::from_str(check_val.as_str().ok_or("checksum value was not a string")?)
                                .map_err(|_| "checksum value could not parse as u32")?;
                        let checksum = book.checksum();
                        if checksum != expected_checksum {
                            log::error!("Error: checksum mismatch, book is out of sync.");
                            book.checksum_failed = true;
                            drop(book);
                            self.subscription_tracker.get_book(pair.to_string()).needs_unsubscribe = true;
                            return Err("checksum mismatch");
                        }
                    }
                }
            } else {
                return Err("update had no usable data");
            }
            book.last_update = Some(Instant::now());
            Ok(())
        } else if self.subscription_tracker.is_ohlc_channel(channel_name) {
            // This looks like an ohlc message. The last item should be the asset pair
            let pair = array
                .last()
                .ok_or("index invalid")?
                .as_str()
                .ok_or("ohlc message did not have asset pair string as last item")?;

            // Check if this matches an ohlc subscription
            let sub = self.subscription_tracker.get_ohlc(pair.to_string());
            if !sub.status.is_subscribed() {
                return Err("unexpected ohlc message, not subscribed");
            }

            let mut lk = self
                .output
                .ohlc
                .get(pair)
                .ok_or("unexpected asset pair update -- check asset pair name")?
                .lock()
                .expect("mutex poisoned");

            let data = array[1].as_array().ok_or("expected one candle, an array")?;

            lk.reserve(1);

            if data.len() < 9 {
                return Err("Expected at least 9 entries in the array");
            }

            let epoc_last_str = data[0].as_str().ok_or("expected epoc_last to be a str")?;
            let epoc_end_str = data[1].as_str().ok_or("expected epoc_end to be a str")?;
            let open_str = data[2].as_str().ok_or("expected open to be a str")?;
            let high_str = data[3].as_str().ok_or("expected high to be a str")?;
            let low_str = data[4].as_str().ok_or("expected low to be a str")?;
            let close_str = data[5].as_str().ok_or("expected close to be a str")?;
            let vwap_str = data[6].as_str().ok_or("expected vwap to be a str")?;
            let volume_str = data[7].as_str().ok_or("expected volume to be a str")?;

            let epoc_last = Decimal::from_str(epoc_last_str).map_err(|_| "could not parse epoc_last")?;
            let epoc_end = Decimal::from_str(epoc_end_str).map_err(|_| "could not parse epoc_end")?;
            let open = Decimal::from_str(open_str).map_err(|_| "could not parse open")?;
            let high = Decimal::from_str(high_str).map_err(|_| "could not parse high")?;
            let low = Decimal::from_str(low_str).map_err(|_| "could not parse low")?;
            let close = Decimal::from_str(close_str).map_err(|_| "could not parse close")?;
            let vwap = Decimal::from_str(vwap_str).map_err(|_| "could not parse vwap")?;
            let volume = Decimal::from_str(volume_str).map_err(|_| "could not parse volume")?;

            lk.push(Candle {
                epoc_last,
                epoc_end,
                open,
                high,
                low,
                close,
                vwap,
                volume,
            });

            Ok(())
        } else {
            Err("unexpected channel name")
        }
    }

    fn handle_system_status(&mut self, map: serde_json::Map<String, Value>) -> Result<(), &'static str> {
        let status = SystemStatus::from_str(
            map.get("status")
                .ok_or("missing status field")?
                .as_str()
                .ok_or("status was not a string")?,
        )?;
        *self.output.system_status.lock().expect("mutex poisoned") = Some(status);
        Ok(())
    }

    fn handle_add_order_status(&mut self, map: serde_json::Map<String, Value>) -> Result<(), &'static str> {
        let req_id = map
            .get("reqid")
            .ok_or("missing req_id field")?
            .as_u64()
            .ok_or("reqid wasnt an integer")?;
        let sender = self
            .add_order_result_senders
            .remove(&req_id)
            .ok_or("unknown add_order reqid")?;
        let status = map
            .get("status")
            .ok_or("missing status field")?
            .as_str()
            .ok_or("status wasnt a string")?;
        if status == "ok" {
            // tx_id is omitted when validate=true
            let tx_id = map
                .get("txid")
                .map(|val| val.as_str().ok_or("txid wasnt a string"))
                .transpose()?
                .unwrap_or_default();
            drop(sender.send(Ok(tx_id.to_string())));
            Ok(())
        } else if status == "error" {
            let err_msg = map
                .get("errorMessage")
                .ok_or("missing errorMessage field")?
                .as_str()
                .ok_or("errorMessage wasnt a string")?;
            log::error!("add_order: {}", err_msg);
            drop(sender.send(Err(err_msg.to_string())));
            Ok(())
        } else {
            log::error!("unexpected status: {}", status);
            drop(sender.send(Err(format!("unexpected status: {}", status))));
            Err("unexpected status")
        }
    }

    fn handle_cancel_order_status(&mut self, map: serde_json::Map<String, Value>) -> Result<(), &'static str> {
        let req_id = map
            .get("reqid")
            .ok_or("missing req_id field")?
            .as_u64()
            .ok_or("reqid wasnt an integer")?;
        let sender = if let Some(sender) = self.cancel_order_result_senders.remove(&req_id) {
            sender
        } else {
            log::debug!(
                "unknown cancel_order reqid ({})\nThis is not always a problem, if a cancel order is placed for multiple orders we only return the first result",
                req_id
            );
            return Ok(());
        };

        let status = map
            .get("status")
            .ok_or("missing status field")?
            .as_str()
            .ok_or("status wasnt a string")?;
        if status == "ok" {
            drop(sender.send(Ok(())));
            Ok(())
        } else if status == "error" {
            let err_msg = map
                .get("errorMessage")
                .ok_or("missing errorMessage field")?
                .as_str()
                .ok_or("errorMessage wasnt a string")?;
            log::error!("cancel_order: {}", err_msg);
            drop(sender.send(Err(err_msg.to_string())));
            Ok(())
        } else {
            log::error!("unexpected status: {}", status);
            drop(sender.send(Err(format!("unexpected status: {}", status))));
            Err("unexpected status")
        }
    }

    fn handle_cancel_all_orders_status(&mut self, map: serde_json::Map<String, Value>) -> Result<(), &'static str> {
        let req_id = map
            .get("reqid")
            .ok_or("missing req_id field")?
            .as_u64()
            .ok_or("reqid wasnt an integer")?;
        let sender = self
            .cancel_all_orders_result_senders
            .remove(&req_id)
            .ok_or("unknown cancel_all_orders reqid")?;
        let status = map
            .get("status")
            .ok_or("missing status field")?
            .as_str()
            .ok_or("status wasnt a string")?;
        if status == "ok" {
            let count = map
                .get("count")
                .ok_or("missing count field")?
                .as_u64()
                .ok_or("count wasnt an integer")?;
            drop(sender.send(Ok(count)));
            Ok(())
        } else if status == "error" {
            let err_msg = map
                .get("errorMessage")
                .ok_or("missing errorMessage field")?
                .as_str()
                .ok_or("errorMessage wasnt a string")?;
            log::error!("cancel_all_orders: {}", err_msg);
            drop(sender.send(Err(err_msg.to_string())));
            Ok(())
        } else {
            log::error!("unexpected status: {}", status);
            drop(sender.send(Err(format!("unexpected status: {}", status))));
            Err("unexpected status")
        }
    }
}

impl Drop for KrakenWsClient {
    fn drop(&mut self) {
        self.output.stream_closed.store(true, Ordering::SeqCst);
    }
}

/// Object which tracks the status of our various subscriptions to Kraken,
/// including both, what Kraken said the current status is, and, when we last tried to
/// change it.
#[derive(Default, Clone, Debug)]
struct SubscriptionTracker {
    /// A map from asset-pairs to book subscription states
    book_subscriptions: HashMap<String, SubscriptionState>,
    /// Known book channel names
    book_channels: HashSet<String>,
    /// A map from asset-pairs to ohlc subscription states
    ohlc_subscriptions: HashMap<String, SubscriptionState>,
    /// Known ohlc channel names
    ohlc_channels: HashSet<String>,
    /// A map from asset-pairs to trade subscription states
    trade_subscriptions: HashMap<String, SubscriptionState>,
    /// Subscription state of the openOrders channel
    open_orders: SubscriptionState,
}

impl SubscriptionTracker {
    pub fn is_book_channel(&self, book_channel: &str) -> bool {
        self.book_channels.contains(book_channel)
    }

    pub fn add_book_channel(&mut self, book_channel: String) {
        self.book_channels.insert(book_channel);
    }

    pub fn get_book(&mut self, asset_pair: String) -> &mut SubscriptionState {
        self.book_subscriptions.entry(asset_pair).or_default()
    }

    pub fn is_ohlc_channel(&self, ohlc_channel: &str) -> bool {
        self.ohlc_channels.contains(ohlc_channel)
    }

    pub fn add_ohlc_channel(&mut self, ohlc_channel: String) {
        self.ohlc_channels.insert(ohlc_channel);
    }

    pub fn get_ohlc(&mut self, asset_pair: String) -> &mut SubscriptionState {
        self.ohlc_subscriptions.entry(asset_pair).or_default()
    }

    pub fn get_trade(&mut self, asset_pair: String) -> &mut SubscriptionState {
        self.trade_subscriptions.entry(asset_pair).or_default()
    }

    pub fn get_open_orders(&mut self) -> &mut SubscriptionState {
        &mut self.open_orders
    }
}

#[derive(Default, Clone, Debug)]
struct SubscriptionState {
    /// The last status that Kraken reported for this subscription
    status: SubscriptionStatus,
    /// The last request that we made to Kraken, and when
    last_request: Option<(SubscriptionStatus, Instant)>,
    /// A note to ourselves that we intend to unsubscribe and resubscribe due
    /// to an error that we detected
    needs_unsubscribe: bool,
    /// A sequence number, for those subscriptions that use it.
    /// Starts at 0 when status is subscribed, None when unsubscribed.
    /// Only user-data subscriptions, like openOrders and ownTrades, carry sequence numbers.
    /// When the numbers don't increment from 1, it indicates that we missed a message somehow,
    /// and we should resubscribe, or reconnect.
    sequence_number: Option<u64>,
}

impl SubscriptionState {
    /// Create a new subscription state with the given status flag
    pub fn new(status: SubscriptionStatus) -> Self {
        Self {
            status,
            last_request: None,
            needs_unsubscribe: false,
            sequence_number: if status.is_subscribed() { Some(0) } else { None },
        }
    }

    /// Check a sequence number against what we have recorded in the tracker
    pub fn check_sequence_number(&mut self, new_sequence_number: u64) -> Result<(), &'static str> {
        let Some(expected_sequence_number) = self.sequence_number.as_mut() else {
            return Err("unexpected message (no sequence number expected for this channel right now)");
        };

        if *expected_sequence_number + 1 != new_sequence_number {
            // We need to try to resubscribe to this channel now
            self.needs_unsubscribe = true;
            return Err("sequence number mismatch");
        }
        *expected_sequence_number += 1;
        Ok(())
    }

    /// Check if we tried to change the status "recently" meaning within
    /// a certain number of seconds. If so then we should back off and wait
    /// rather than try to change it again.
    pub fn tried_to_change_recently(&self) -> bool {
        self.last_request
            .map(|(stat, time)| stat != self.status && time + SUBSCRIPTION_CHANGE_BACKOFF > Instant::now())
            .unwrap_or(false)
    }
}
