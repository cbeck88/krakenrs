//! An interface for getting data from Kraken websockets API, while another thread manages
//! the updates from the websockets connection.

mod conn;
use conn::{ApiResults, KrakenWsClient};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;

pub use conn::{BookData, Error as WsError, KrakenWsConfig, SystemStatus};

/// A handle to Kraken websockets API feeds
pub struct KrakenWsApi {
    // The worker thread that is consuming kraken api messages
    worker_thread: Option<thread::JoinHandle<()>>,
    // Handle to ask the worker thread to stop
    stop_requested: Arc<AtomicBool>,
    // Handle to the output of the worker thread
    output: Arc<ApiResults>,
}

impl KrakenWsApi {
    /// Create a new web sockets connection to Kraken and subscribe to
    /// specified channels
    pub fn new(src: KrakenWsConfig) -> Result<Self, WsError> {
        let stop_requested = Arc::new(AtomicBool::default());
        let thread_stop_requested = stop_requested.clone();
        let (mut client, output) = KrakenWsClient::new(src)?;
        let worker_thread = Some(thread::spawn(move || loop {
            if let Err(err) = client.update() {
                eprintln!("KrakenWsClient: {}", err);
            }
            if let Err(err) = client.maybe_resubscribe() {
                eprintln!("KrakenWsClient: Error resubscribing: {}", err);
            }
            if thread_stop_requested.load(Ordering::SeqCst) {
                if let Err(err) = client.close() {
                    eprintln!("KrakenWsClient: Error closing: {}", err);
                }
                return;
            }
        }));
        Ok(Self {
            worker_thread,
            stop_requested,
            output,
        })
    }

    /// Get the system status
    pub fn system_status(&self) -> Option<SystemStatus> {
        self.output
            .system_status
            .lock()
            .expect("mutex poisoned")
            .clone()
    }

    /// Get book data that we have subscribed to
    pub fn get_book(&self, pair: &str) -> BookData {
        self.output
            .book
            .get(pair)
            .expect("unknown asset pair")
            .lock()
            .expect("mutex poisoned")
            .clone()
    }

    /// Check if the stream is closed
    pub fn stream_closed(&self) -> bool {
        self.output.stream_closed.load(Ordering::SeqCst)
    }
}

impl Drop for KrakenWsApi {
    fn drop(&mut self) {
        if let Some(worker_thread) = self.worker_thread.take() {
            self.stop_requested.store(true, Ordering::SeqCst);
            worker_thread.join().expect("Could not join thread");
        }
    }
}
