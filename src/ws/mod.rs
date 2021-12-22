//! An interface for getting data from Kraken websockets API, while another thread manages
//! the updates from the websockets connection.
//!
//! This follows the pattern of bridging async code into a sync interface
//! See also: https://tokio.rs/tokio/topics/bridging
//! and the reqwest::blocking module

use futures::stream::StreamExt;
use std::sync::{atomic::Ordering, Arc};
use std::{
    collections::{BTreeMap, HashMap},
    thread,
};
use tokio::{runtime, sync::mpsc};

mod conn;
pub use conn::{Error, KrakenPrivateWsConfig, KrakenWsClient, KrakenWsConfig, WsAPIResults};

mod types;
pub use types::{BookData, BookEntry};

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
                                                log::error!("KrakenWsClient: error, closing stream: {}", err);
                                                drop(client.close());
                                                return;
                                            }
                                        }
                                    }
                                    None => {
                                        log::warn!("KrakenWsClient: stream closed by kraken");
                                        drop(client.close());
                                        return;
                                    }
                                }
                            }
                            msg = receiver.recv() => {
                                match msg {
                                    None | Some(LocalRequest::Stop) => {
                                        drop(client.close());
                                        return;
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

    /// Get latest openOrder data
    pub fn get_open_orders(&self) -> HashMap<String, OrderInfo> {
        self.output.open_orders.lock().expect("mutex poisoned").clone()
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
}

impl Drop for KrakenWsAPI {
    fn drop(&mut self) {
        if let Some(worker_thread) = self.worker_thread.take() {
            drop(self.sender.send(LocalRequest::Stop));
            worker_thread.join().expect("Could not join thread");
        }
    }
}

/// A request made from the local handle (KrakenWsApi) to
/// the thread perfoming the websockets operations.
enum LocalRequest {
    Stop,
}
