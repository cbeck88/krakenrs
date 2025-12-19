//! An interface for getting data from Kraken websockets API, while another thread manages
//! the updates from the websockets connection.
//!
//! This follows the pattern of bridging async code into a sync interface
//! See also: <https://tokio.rs/tokio/topics/bridging>
//! and the `reqwest::blocking` module

use crate::{LimitOrder, MarketOrder};
use futures::stream::StreamExt;
use std::sync::{Arc, atomic::Ordering};
use std::{
    collections::{BTreeMap, HashMap},
    thread,
    time::{Duration, Instant},
};
use tokio::{
    runtime,
    sync::{mpsc, oneshot},
    time,
};

mod config;
pub use config::{KrakenWsConfig, KrakenWsConfigBuilder};

mod conn;
pub use conn::{Error, KrakenWsClient, WsAPIResults};

mod types;
pub use types::{BookData, BookEntry, Candle, PublicTrade};

mod messages;
pub use messages::*;

/// A handle to Kraken websockets API feeds
///
/// This is a sync API, but under the hood it contains a thread driving a small
/// tokio runtime
pub struct KrakenWsAPI {
    // The worker thread that is consuming kraken api messages
    worker_thread: Option<thread::JoinHandle<()>>,
    // Sender object to send messages to the worker thread
    sender: mpsc::UnboundedSender<LocalRequest>,
    // Handle to the output of the worker thread
    output: Arc<WsAPIResults>,
}

impl KrakenWsAPI {
    /// Create a new web sockets connection to Kraken and subscribe to
    /// specified channels
    ///
    /// Note: This is the same as using `TryFrom::try_from` to construct an instance
    ///
    /// Note: This call attempts to fail fast if a websockets connection cannot be established,
    /// so it will block the current thread on that and return an error if connection fails.
    /// If you are using the tokio multi-threaded runtime, you must call this from a blocking thread,
    /// or the runtime will detect this and panic. You may wrap it in `task::spawn_blocking` or similar.
    pub fn new(src: KrakenWsConfig) -> Result<Self, Error> {
        // Build the runtime for the new thread.
        //
        // The runtime is created before spawning the thread
        // to more cleanly forward errors if the `unwrap()`
        // panics.
        let rt = runtime::Builder::new_current_thread().enable_all().build().unwrap();

        let (mut client, mut stream, output) = rt.block_on(KrakenWsClient::new(src))?;
        let (sender, mut receiver) = mpsc::unbounded_channel();

        let worker_thread = Some(thread::Builder::new().name("kraken-ws-internal-runtime".into()).spawn(
            move || {
                rt.block_on(async move {
                    // Every second, confirm that we got a heart beat, or send a ping / expect a pong
                    let mut interval = time::interval(Duration::from_secs(1));
                    loop {
                        tokio::select! {
                            stream_result = stream.next() => {
                                match stream_result {
                                    Some(result) => {
                                        match client.update(result) {
                                            Ok(()) => {
                                                // Maybe adjust subscriptions, closing corrupted subscriptions,
                                                // and resubscribing to any subscriptions that are missing for a while
                                                // to any subscriptions that were canceled
                                                client.check_subscriptions().await;
                                            }
                                            Err(err) => {
                                                log::error!("error, closing stream: {}", err);
                                                drop(client.close().await);
                                                return;
                                            }
                                        }
                                    }
                                    None => {
                                        log::warn!("stream closed by kraken");
                                        drop(client.close().await);
                                        return;
                                    }
                                }
                            }
                            msg = receiver.recv() => {
                                match msg {
                                    None | Some(LocalRequest::Stop) => {
                                        drop(client.close().await);
                                        return;
                                    }
                                    Some(LocalRequest::AddOrder{request, result_sender}) => {
                                        if let Err(err) = client.add_order(request, result_sender).await {
                                            log::error!("error submitting an order, closing stream: {}", err);
                                            drop(client.close().await);
                                            return;
                                        }
                                    }
                                    Some(LocalRequest::CancelOrder{tx_id, result_sender}) => {
                                        if let Err(err) = client.cancel_order(tx_id, result_sender).await {
                                            log::error!("error canceling an order, closing stream: {}", err);
                                            drop(client.close().await);
                                            return;
                                        }
                                    }
                                    Some(LocalRequest::CancelAllOrders{result_sender}) => {
                                        if let Err(err) = client.cancel_all_orders(result_sender).await {
                                            log::error!("error canceling all orders, closing stream: {}", err);
                                            drop(client.close().await);
                                            return;
                                        }
                                    }
                                }
                            }
                            _ = interval.tick() => {
                                if let Some(time) = client.get_last_message_time() {
                                    // If we haven't heard anything in a while that's bad
                                    // Kraken says they send a heartbeat about every second
                                    let now = Instant::now();
                                    if time + Duration::from_secs(2) < now {
                                        // Check if we earlier sent a ping
                                        if let Some(ping_time) = client.get_last_outstanding_ping_time() {
                                            if ping_time + Duration::from_secs(1) < now {
                                                log::error!("Kraken did not respond to ping, closing stream");
                                                drop(client.close().await);
                                                return;
                                            }
                                        } else {
                                            // There is no outstanding ping, let's send a ping
                                            if let Err(err) = client.ping().await {
                                                log::error!("error sending ping, closing stream: {}", err);
                                                drop(client.close().await);
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                })
            },
        )?);
        Ok(Self {
            worker_thread,
            sender,
            output,
        })
    }

    /// Create a new web sockets connection to Kraken and subscribe to
    /// specified channels (async version)
    ///
    /// This is the async version that should be used when you are already in an async context.
    /// It establishes the websockets connection and spawns a background thread to manage updates.
    pub async fn new_async(src: KrakenWsConfig) -> Result<Self, Error> {
        let (mut client, mut stream, output) = KrakenWsClient::new(src).await?;
        let (sender, mut receiver) = mpsc::unbounded_channel();

        let worker_thread = Some(thread::Builder::new().name("kraken-ws-internal-runtime".into()).spawn(
            move || {
                let rt = runtime::Builder::new_current_thread().enable_all().build().unwrap();
                rt.block_on(async move {
                    // Every second, confirm that we got a heart beat, or send a ping / expect a pong
                    let mut interval = time::interval(Duration::from_secs(1));
                    loop {
                        tokio::select! {
                            stream_result = stream.next() => {
                                match stream_result {
                                    Some(result) => {
                                        match client.update(result) {
                                            Ok(()) => {
                                                // Maybe adjust subscriptions, closing corrupted subscriptions,
                                                // and resubscribing to any subscriptions that are missing for a while
                                                // to any subscriptions that were canceled
                                                client.check_subscriptions().await;
                                            }
                                            Err(err) => {
                                                log::error!("error, closing stream: {}", err);
                                                drop(client.close().await);
                                                return;
                                            }
                                        }
                                    }
                                    None => {
                                        log::warn!("stream closed by kraken");
                                        drop(client.close().await);
                                        return;
                                    }
                                }
                            }
                            msg = receiver.recv() => {
                                match msg {
                                    None | Some(LocalRequest::Stop) => {
                                        drop(client.close().await);
                                        return;
                                    }
                                    Some(LocalRequest::AddOrder{request, result_sender}) => {
                                        if let Err(err) = client.add_order(request, result_sender).await {
                                            log::error!("error submitting an order, closing stream: {}", err);
                                            drop(client.close().await);
                                            return;
                                        }
                                    }
                                    Some(LocalRequest::CancelOrder{tx_id, result_sender}) => {
                                        if let Err(err) = client.cancel_order(tx_id, result_sender).await {
                                            log::error!("error canceling an order, closing stream: {}", err);
                                            drop(client.close().await);
                                            return;
                                        }
                                    }
                                    Some(LocalRequest::CancelAllOrders{result_sender}) => {
                                        if let Err(err) = client.cancel_all_orders(result_sender).await {
                                            log::error!("error canceling all orders, closing stream: {}", err);
                                            drop(client.close().await);
                                            return;
                                        }
                                    }
                                }
                            }
                            _ = interval.tick() => {
                                if let Some(time) = client.get_last_message_time() {
                                    // If we haven't heard anything in a while that's bad
                                    // Kraken says they send a heartbeat about every second
                                    let now = Instant::now();
                                    if time + Duration::from_secs(2) < now {
                                        // Check if we earlier sent a ping
                                        if let Some(ping_time) = client.get_last_outstanding_ping_time() {
                                            if ping_time + Duration::from_secs(1) < now {
                                                log::error!("Kraken did not respond to ping, closing stream");
                                                drop(client.close().await);
                                                return;
                                            }
                                        } else {
                                            // There is no outstanding ping, let's send a ping
                                            if let Err(err) = client.ping().await {
                                                log::error!("error sending ping, closing stream: {}", err);
                                                drop(client.close().await);
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                })
            },
        )?);
        Ok(Self {
            worker_thread,
            sender,
            output,
        })
    }

    /// Get the system status
    pub fn system_status(&self) -> Option<SystemStatus> {
        self.output.system_status.lock().expect("mutex poisoned").clone()
    }

    /// Get all latest book data that we have subscribed to
    pub fn get_all_books(&self) -> BTreeMap<String, BookData> {
        self.output
            .book
            .iter()
            .map(|(asset_pair, lock)| (asset_pair.clone(), lock.lock().expect("mutex poisoned").clone()))
            .collect()
    }

    /// Get latest book data that we have subscribed to, for an individual book
    pub fn get_book(&self, asset_pair: &str) -> Option<BookData> {
        self.output
            .book
            .get(asset_pair)
            .map(|lock| lock.lock().expect("mutex poisoned").clone())
    }

    /// Get the most recent trades that we have seen, for an individual asset pair
    /// Note that these can only be retrieved once and are not delivered to the next consumer.
    ///
    /// Returns None only if the asset pair is unknown, which is usually a logic error.
    pub fn get_ohlc(&self, asset_pair: &str) -> Option<Vec<Candle>> {
        self.output.ohlc.get(asset_pair).map(|lock| {
            let mut lk = lock.lock().expect("mutex poisoned");
            let result = lk.clone();
            lk.clear(); // note, this doesn't reduce the capacity
            result
        })
    }

    /// Get the most recent trades that we have seen, for an individual asset pair
    /// Note that these can only be retrieved once and are not delivered to the next consumer.
    ///
    /// Returns None only if the asset pair is unknown, which is usually a logic error.
    pub fn get_trades(&self, asset_pair: &str) -> Option<Vec<PublicTrade>> {
        self.output.trades.get(asset_pair).map(|lock| {
            let mut lk = lock.lock().expect("mutex poisoned");
            let result = lk.clone();
            lk.clear(); // note, this doesn't reduce the capacity
            result
        })
    }

    /// Get latest openOrder data
    pub fn get_open_orders(&self) -> HashMap<String, OrderInfo> {
        self.output.open_orders.lock().expect("mutex poisoned").clone()
    }

    /// Get latest ownTrades data
    /// Note that each trade can only be retrieved once and is not delivered to the next consumer.
    pub fn get_own_trades(&self) -> Vec<OwnTrade> {
        let mut lk = self.output.own_trades.lock().expect("mutex poisoned");
        let result = lk.clone();
        lk.clear(); // note, this doesn't reduce the capacity
        result
    }

    /// Check if the stream is closed. If so then we should abandon this
    /// instance of KrakenWsAPI and create a new one in order to reconnect.
    ///
    /// Note Kraken's advisory:
    /// Cloudflare imposes a connection/re-connection rate limit (per IP address) of approximately 150 attempts per rolling 10 minutes. If this is exceeded, the IP is banned for 10 minutes.
    /// Recommended reconnection behaviour is to (1) attempt reconnection instantly up to a handful of times if the websocket is dropped randomly during normal operation but (2) after maintenance or extended downtime, attempt to reconnect no more quickly than once every 5 seconds. There is no advantage to reconnecting more rapidly after maintenance during cancel_only mode.
    pub fn stream_closed(&self) -> bool {
        self.output.stream_closed.load(Ordering::SeqCst)
    }

    /// Submit a market order over the websockets connection.
    /// This must be a private connection configured with the auth token.
    ///
    /// Arguments:
    /// market_order: The market order to place
    /// user_ref_id: The user-ref-id to associate to this order. Orders may be filtered or canceled by user-ref-id.
    /// validate: If true, we just validate that the order was well formed and the order doesn't actually hit the books.
    ///
    /// Returns:
    /// A oneshot::Reciever which yields either the TxID for the placed order, or an error message from kraken.
    /// The Receiver produces no value if the order could not be successfully placed, and this will be logged.
    /// The Receiver may be dropped if you don't care about the errors -- these error messages will be logged regardless.
    /// The return value will be None if the stream is already closed.
    pub fn add_market_order(
        &self,
        market_order: MarketOrder,
        user_ref_id: Option<i32>,
        validate: bool,
    ) -> Option<oneshot::Receiver<Result<String, String>>> {
        let (result_sender, result_receiver) = oneshot::channel();
        let request = AddOrderRequest {
            ordertype: OrderType::Market,
            bs_type: market_order.bs_type.into(),
            volume: market_order.volume,
            pair: market_order.pair,
            price: Default::default(),
            oflags: market_order.oflags.into_iter().map(OrderFlag::from).collect(),
            userref: user_ref_id,
            validate,
            ..Default::default()
        };
        if self
            .sender
            .send(LocalRequest::AddOrder { request, result_sender })
            .is_ok()
        {
            Some(result_receiver)
        } else {
            None
        }
    }

    /// Submit a limit order over the websockets connection.
    /// This must be a private connection configured with the auth token.
    ///
    /// Arguments:
    /// limit_order: The order order to place
    /// user_ref_id: The user-ref-id to associate to this order. Orders may be filtered or canceled by user-ref-id.
    /// validate: If true, we just validate that the order was well formed and the order doesn't actually hit the books.
    ///
    /// Returns:
    /// A oneshot::Reciever which yields either the TxID for the placed order, or an error message from kraken.
    /// The Receiver produces no value if the order could not be successfully placed, and this will be logged.
    /// The Receiver may be dropped if you don't care about the errors -- these error messages will be logged regardless.
    /// The return value will be None if the stream is already closed.
    pub fn add_limit_order(
        &self,
        limit_order: LimitOrder,
        user_ref_id: Option<i32>,
        validate: bool,
    ) -> Option<oneshot::Receiver<Result<String, String>>> {
        let (result_sender, result_receiver) = oneshot::channel();
        let request = AddOrderRequest {
            ordertype: OrderType::Limit,
            bs_type: limit_order.bs_type.into(),
            volume: limit_order.volume,
            pair: limit_order.pair,
            price: limit_order.price,
            oflags: limit_order.oflags.into_iter().map(OrderFlag::from).collect(),
            userref: user_ref_id,
            validate,
            ..Default::default()
        };
        if self
            .sender
            .send(LocalRequest::AddOrder { request, result_sender })
            .is_ok()
        {
            Some(result_receiver)
        } else {
            None
        }
    }

    /// Submit a request to cancel an order over the websockets connection.
    /// This must be a private connection configured with the auth token.
    ///
    /// Arguments:
    /// tx_id: The TxId associated to an order, or, a user-ref-id
    ///
    /// Returns:
    /// A oneshot::Reciever which yields either Ok on success canceling, or an error message from kraken.
    /// The Receiver produces no value if the request could not be successfully placed, and this will be logged.
    /// The Receiver may be dropped if you don't care about the errors -- these error messages will be logged regardless.
    /// The return value will be None if the stream is already closed.
    pub fn cancel_order(&self, tx_id: String) -> Option<oneshot::Receiver<Result<(), String>>> {
        let (result_sender, result_receiver) = oneshot::channel();
        if self
            .sender
            .send(LocalRequest::CancelOrder { tx_id, result_sender })
            .is_ok()
        {
            Some(result_receiver)
        } else {
            None
        }
    }

    /// Submit a request to cancel all orders over the websockets connection.
    /// This must be a private connection configured with the auth token.
    ///
    /// Returns:
    /// A oneshot::Reciever which yields either Ok and a count of canceled orders, or an error message from kraken.
    /// The Receiver produces no value if the request could not be successfully placed, and this will be logged.
    /// The Receiver may be dropped if you don't care about the errors -- these error messages will be logged regardless.
    /// The return value will be None if the stream is already closed.
    pub fn cancel_all_orders(&self) -> Option<oneshot::Receiver<Result<u64, String>>> {
        let (result_sender, result_receiver) = oneshot::channel();
        if self
            .sender
            .send(LocalRequest::CancelAllOrders { result_sender })
            .is_ok()
        {
            Some(result_receiver)
        } else {
            None
        }
    }
}

impl Drop for KrakenWsAPI {
    fn drop(&mut self) {
        if let Some(worker_thread) = self.worker_thread.take() {
            drop(self.sender.send(LocalRequest::Stop));
            worker_thread.join().expect("Could not join thread");
        }
    }
}

impl std::convert::TryFrom<KrakenWsConfig> for KrakenWsAPI {
    type Error = Error;
    fn try_from(src: KrakenWsConfig) -> Result<KrakenWsAPI, Error> {
        KrakenWsAPI::new(src)
    }
}

/// A request made from the local handle (KrakenWsAPI) to
/// the thread perfoming the websockets operations.
enum LocalRequest {
    /// Requests to stop the worker thread and close the connection gracefully
    Stop,
    /// Requests to add an order to the order book
    AddOrder {
        request: AddOrderRequest,
        result_sender: oneshot::Sender<Result<String, String>>,
    },
    /// Requests to cancel one of our orders
    CancelOrder {
        tx_id: String,
        result_sender: oneshot::Sender<Result<(), String>>,
    },
    /// Requests to cancel all of our orders
    CancelAllOrders {
        result_sender: oneshot::Sender<Result<u64, String>>,
    },
}
