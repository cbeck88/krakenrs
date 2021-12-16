//! An interface for getting data from Kraken websockets API, while another thread manages
//! the updates from the websockets connection.

mod conn;
use conn::{ApiResults, BookData, Error, KrakenWsClient, KrakenWsConfig};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// A handle to Kraken websockets API feeds
pub struct KrakenWsApi {
    // The worker thread that is consuming kraken api messages
    worker_thread: thread::JoinHandle<()>,
    // Handle to the output of the worker thread
    output: Arc<ApiResults>,
}

impl KrakenWsApi {
    /// Create a new web sockets connection to Kraken and subscribe to
    /// specified channels
    pub fn new(src: KrakenWsConfig) -> Result<Self, Error> {
        let (mut client, output) = KrakenWsClient::new(src)?;
        let worker_thread = thread::spawn(move || loop {
            if let Err(err) = client.update() {
                eprintln!("KrakenWsClient: {}", err);
            }
            thread::sleep(Duration::from_millis(100));
        });
        Ok(Self {
            worker_thread,
            output,
        })
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
}
