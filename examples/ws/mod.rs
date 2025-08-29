use displaydoc::Display;
use env_logger::{fmt::Color, Builder, Env};
use futures::executor::block_on;
use krakenrs::{
    ws::{KrakenPrivateWsConfig, KrakenWsAPI, KrakenWsConfig},
    BsType, KrakenCredentials, KrakenRestAPI, KrakenRestConfig, LimitOrder, MarketOrder, OrderFlag,
};
use log::Level;
use std::{
    collections::BTreeSet,
    convert::TryFrom,
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
use structopt::StructOpt;

/// Structure representing parsed command-line arguments to krak-feed executable
#[derive(StructOpt)]
struct KrakFeedConfig {
    #[structopt(subcommand)]
    command: Command,

    /// Credentials file, formatted in json. Required only for private APIs
    #[structopt(parse(from_os_str))]
    creds: Option<PathBuf>,

    /// Whether to pass "validate = true" with any orders (for testing)
    #[structopt(short, long)]
    validate: bool,
}

/// Commands supported by krak-feed executable
#[derive(StructOpt, Display)]
enum Command {
    /// Get websockets feed for one or more asset-pair books
    Book { pairs: Vec<String> },

    /// Get public trades feed for one asset-pair
    Trades { pair: String },

    /// Get websockets feed for own open orders
    OpenOrders {},

    /// Market buy order: {volume} {pair}
    MarketBuy { volume: String, pair: String },

    /// Market sell order: {volume} {pair}
    MarketSell { volume: String, pair: String },

    /// Limit buy order: {volume} {pair} @ {price}
    LimitBuy {
        volume: String,
        pair: String,
        price: String,
    },

    /// Limit sell order: {volume} {pair} @ {price}
    LimitSell {
        volume: String,
        pair: String,
        price: String,
    },

    /// Cancel order: {id}
    CancelOrder { id: String },

    /// Cancel all orders
    CancelAllOrders,
}

static PROCESS_TERMINATING: AtomicBool = AtomicBool::new(false);

// Helper: Get a private websockets connection
fn get_private_websockets_api(creds: &Option<PathBuf>) -> KrakenWsAPI {
    // First get a websockets token
    let mut kc_config = KrakenRestConfig::default();

    // Load credentials from disk if specified
    let creds = creds.as_ref().expect("Missing credentials");

    log::info!("Credentials path: {:?}", creds);
    kc_config.creds = KrakenCredentials::load_json_file(creds).expect("credential file error");

    let api = KrakenRestAPI::try_from(kc_config).expect("could not create kraken api");
    let token = api
        .get_websockets_token()
        .expect("could not get websockets token")
        .token;

    let ws_config = KrakenWsConfig {
        private: Some(KrakenPrivateWsConfig {
            token,
            subscribe_open_orders: true,
        }),
        ..Default::default()
    };
    KrakenWsAPI::new(ws_config).expect("could not connect to websockets api")
}

pub fn main() {
    // Default to INFO log level for everything if we do not have an explicit
    // setting.
    Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let mut style = buf.style();

            let color = match record.level() {
                Level::Error => Color::Red,
                Level::Warn => Color::Yellow,
                Level::Info => Color::Green,
                Level::Debug => Color::Cyan,
                Level::Trace => Color::Magenta,
            };
            style.set_color(color).set_bold(true);

            writeln!(
                buf,
                "{} {} [{} {}:{}] {}",
                chrono::Utc::now(),
                style.value(record.level()),
                record.module_path().unwrap_or("?"),
                record.file().unwrap_or("?"),
                record.line().unwrap_or(0),
                record.args(),
            )
        })
        .init();

    let config = KrakFeedConfig::from_args();

    match config.command {
        Command::Book { pairs } => {
            let ws_config = KrakenWsConfig {
                subscribe_book: pairs,
                ..Default::default()
            };
            let api = KrakenWsAPI::new(ws_config).expect("could not connect to websockets api");

            let mut prev = api.get_all_books();

            loop {
                let next = api.get_all_books();

                if next != prev {
                    for (pair, book_data) in &next {
                        println!("{} bids:", pair);
                        for (price, entry) in book_data.bid.iter() {
                            println!("{}\t\t{}", price, entry.volume);
                        }
                        println!("{} asks:", pair);
                        for (price, entry) in book_data.ask.iter() {
                            println!("{}\t\t{}", price, entry.volume);
                        }
                        println!();
                        if book_data.checksum_failed {
                            println!("Checksum failed, aborting");
                            return;
                        }
                    }
                    prev = next;
                }

                if api.stream_closed() {
                    log::info!("Stream closed");
                    return;
                }
            }
        }
        Command::Trades { pair } => {
            let ws_config = KrakenWsConfig {
                subscribe_trades: vec![pair.clone()],
                ..Default::default()
            };
            let api = KrakenWsAPI::new(ws_config).expect("could not connect to websockets api");

            loop {
                let next = api.get_trades(&pair).expect("asset pair should be known");

                for t in next {
                    let price = t.price;
                    let volume = t.volume;
                    let side = t.side;
                    let timestamp = t.timestamp;
                    println!("{side} {volume} {pair} @ {price} ({timestamp})");
                }

                if api.stream_closed() {
                    log::info!("Stream closed");
                    return;
                }
            }
        }
        Command::OpenOrders {} => {
            let api = get_private_websockets_api(&config.creds);

            let mut prev = api.get_open_orders();

            loop {
                let next = api.get_open_orders();

                if next != prev {
                    println!("Orders:");
                    println!("{}", serde_json::to_string_pretty(&next).unwrap());
                    println!();
                    prev = next;
                }

                if api.stream_closed() {
                    log::info!("Stream closed");
                    return;
                }

                if PROCESS_TERMINATING.load(Ordering::SeqCst) {
                    log::debug!("Process terminating");
                    return;
                }
            }
        }
        Command::MarketBuy { volume, pair } => {
            let api = get_private_websockets_api(&config.creds);

            let result = api
                .add_market_order(
                    MarketOrder {
                        bs_type: BsType::Buy,
                        volume,
                        pair,
                        oflags: Default::default(),
                    },
                    None,
                    config.validate,
                )
                .expect("api call failed");
            match block_on(result).expect("Failed to submit order") {
                Ok(tx_id) => log::info!("Success: tx_id = {}", tx_id),
                Err(err) => log::error!("Failed: {}", err),
            }
        }
        Command::MarketSell { volume, pair } => {
            let api = get_private_websockets_api(&config.creds);

            let result = api
                .add_market_order(
                    MarketOrder {
                        bs_type: BsType::Sell,
                        volume,
                        pair,
                        oflags: Default::default(),
                    },
                    None,
                    config.validate,
                )
                .expect("api call failed");
            match block_on(result).expect("Failed to submit order") {
                Ok(tx_id) => log::info!("Success: tx_id = {}", tx_id),
                Err(err) => log::error!("Failed: {}", err),
            }
        }
        Command::LimitBuy { volume, pair, price } => {
            let api = get_private_websockets_api(&config.creds);

            let mut oflags = BTreeSet::new();
            oflags.insert(OrderFlag::Post);
            let result = api
                .add_limit_order(
                    LimitOrder {
                        bs_type: BsType::Buy,
                        volume,
                        pair,
                        price,
                        oflags,
                    },
                    None,
                    config.validate,
                )
                .expect("api call failed");
            match block_on(result).expect("Failed to submit order") {
                Ok(tx_id) => log::info!("Success: tx_id = {}", tx_id),
                Err(err) => log::error!("Failed: {}", err),
            }
        }
        Command::LimitSell { volume, pair, price } => {
            let api = get_private_websockets_api(&config.creds);

            let mut oflags = BTreeSet::new();
            oflags.insert(OrderFlag::Post);
            let result = api
                .add_limit_order(
                    LimitOrder {
                        bs_type: BsType::Sell,
                        volume,
                        pair,
                        price,
                        oflags,
                    },
                    None,
                    config.validate,
                )
                .expect("api call failed");
            match block_on(result).expect("Failed to submit order") {
                Ok(tx_id) => log::info!("Success: tx_id = {}", tx_id),
                Err(err) => log::error!("Failed: {}", err),
            }
        }
        Command::CancelOrder { id } => {
            let api = get_private_websockets_api(&config.creds);

            let result = api.cancel_order(id).expect("api call failed");
            match block_on(result).expect("Failed to submit request") {
                Ok(()) => log::info!("Success"),
                Err(err) => log::error!("Failed: {}", err),
            }
        }
        Command::CancelAllOrders => {
            let api = get_private_websockets_api(&config.creds);

            let result = api.cancel_all_orders().expect("api call failed");
            match block_on(result).expect("Failed to submit request") {
                Ok(num) => log::info!("Success, {} orders canceled", num),
                Err(err) => log::error!("Failed: {}", err),
            }
        }
    };
}
