use crate::BuilderError;

/// Configuration for the websocket connection and feeds to subscribe to
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct KrakenWsConfig {
    /// Order books to subscribe to
    pub(crate) subscribe_book: Vec<String>,
    /// Depth of order book subscriptions (how many ask/bid entries)
    pub(crate) book_depth: usize,
    /// Public trade streams to subscribe to
    pub(crate) subscribe_trades: Vec<String>,
    /// Optional configuration for private feeds
    pub(crate) private: Option<KrakenPrivateWsConfig>,
}

impl KrakenWsConfig {
    /// Get a builder for the KrakenWsConfig object
    pub fn builder() -> KrakenWsConfigBuilder {
        Default::default()
    }
}

impl Default for KrakenWsConfig {
    fn default() -> Self {
        Self {
            subscribe_book: Default::default(),
            book_depth: 10,
            subscribe_trades: Default::default(),
            private: None,
        }
    }
}

/// Builder for the KrakenWsConfig object
#[derive(Default)]
pub struct KrakenWsConfigBuilder {
    config: KrakenWsConfig,
}

impl KrakenWsConfigBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Websockets names of asset pairs whose order books to subscribe to
    pub fn subscribe_book(mut self, subscribe_book: Vec<String>) -> Self {
        self.config.subscribe_book = subscribe_book;
        self
    }

    /// How many book entries to have on the bid and ask side of book subscriptions.
    /// Will be clamped between 10 and 1000. Defaults to 10.
    pub fn book_depth(mut self, book_depth: usize) -> Self {
        self.config.book_depth = book_depth;
        self
    }

    /// Websockets names of asset pairs whose public trade feeds to subscribe to
    ///
    /// Note: Unlike book and open order info, the queue of received trades will grow unbounded
    /// over time. You must periodically call `KrakenWsAPI::get_trades(...)` or similar to drain
    /// this queue, or your program will face memory exhaustion eventually.
    pub fn subscribe_trades(mut self, subscribe_trades: Vec<String>) -> Self {
        self.config.subscribe_trades = subscribe_trades;
        self
    }

    /// Set the websockets token for this connection. This is required to subscribe
    /// to any private feeds.
    pub fn token(mut self, token: String) -> Self {
        let private = self.config.private.get_or_insert_default();
        private.token = token;
        self
    }

    /// Whether to subscribe to a feed of our own open orders. Note that this is
    /// a private API and requires a websockets token
    pub fn subscribe_open_orders(mut self, subscribe_open_orders: bool) -> Self {
        let private = self.config.private.get_or_insert_default();
        private.subscribe_open_orders = subscribe_open_orders;
        self
    }

    /// Build a valid KrakenWsConfig if possible
    pub fn build(self) -> Result<KrakenWsConfig, BuilderError> {
        if let Some(private) = self.config.private.as_ref() {
            if private.token.is_empty() {
                return Err(BuilderError::MissingWsToken);
            }
        }
        Ok(self.config)
    }
}

/// Configuration for private websockets feeds
#[derive(Clone, Debug, Default)]
pub(crate) struct KrakenPrivateWsConfig {
    /// Authentication token (get from REST API)
    pub(crate) token: String,
    /// If true, subscribe to own orders feed for this account
    pub(crate) subscribe_open_orders: bool,
}
