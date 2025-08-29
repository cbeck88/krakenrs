use core::convert::TryFrom;
use core::fmt::Debug;
use displaydoc::Display;
use env_logger::{fmt::Color, Builder, Env};
use krakenrs::{BsType, KrakenCredentials, KrakenRestAPI, KrakenRestConfig, LimitOrder, MarketOrder, OrderFlag};
use log::Level;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::PathBuf,
};
use structopt::StructOpt;

/// Structure representing parsed command-line arguments to "kraken" executable
#[derive(StructOpt)]
struct KrakenConfig {
    #[structopt(subcommand)]
    command: Command,

    /// Credentials file, formatted in json. Required only for private APIs
    #[structopt(parse(from_os_str))]
    creds: Option<PathBuf>,

    /// Whether to pass "validate = true" with any orders (for testing)
    #[structopt(short, long)]
    validate: bool,
}

/// Commands supported by kraken executable
#[derive(StructOpt, Display)]
enum Command {
    /// Get kraken system time
    Time,
    /// Get kraken system status
    SystemStatus,
    /// Get kraken's asset list
    Assets,
    /// Get kraken's asset pairs info: {pairs:?}
    AssetPairs { pairs: Vec<String> },
    /// Get kraken's ticker info: {pairs:?}
    Ticker { pairs: Vec<String> },
    /// Get account balance
    GetBalance,
    /// Get account trade volume (and fees): {pairs:?}
    GetTradeVolume { pairs: Vec<String> },
    /// Get websockets token
    GetWebSocketsToken,
    /// Get open orders list
    GetOpenOrders,
    /// Cancel order: {id}
    CancelOrder { id: String },
    /// Cancel all orders
    CancelAllOrders,
    /// Cancel all orders after: {timeout}
    CancelAllOrdersAfter { timeout: u64 },
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
}

/// Logs a "pretty printed" json structure on stdout
fn log_value<T: Serialize + Debug>(val: &T) {
    match serde_json::to_string_pretty(val) {
        Ok(pretty) => {
            log::info!("{}", pretty);
        }
        Err(err) => {
            log::error!("Could not pretty-print structure: {:?}: {}", val, err);
        }
    }
}

fn main() {
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

    let config = KrakenConfig::from_args();

    let mut kc_config = KrakenRestConfig::default();

    // Load credentials from disk if specified
    if let Some(creds) = config.creds {
        log::info!("Credentials path: {:?}", creds);
        kc_config.creds = KrakenCredentials::load_json_file(creds).expect("credential file error");
    }

    let api = KrakenRestAPI::try_from(kc_config).expect("could not create kraken api");

    match config.command {
        Command::Time => {
            let result = api.time().expect("api call failed");
            log_value(&result);
        }
        Command::SystemStatus => {
            let result = api.system_status().expect("api call failed");
            log_value(&result);
        }
        Command::Assets => {
            let result = api.assets().expect("api call failed");
            let sorted_result = result.into_iter().collect::<BTreeMap<_, _>>();
            log_value(&sorted_result);
        }
        Command::AssetPairs { pairs } => {
            let result = api.asset_pairs(pairs).expect("api call failed");
            let sorted_result = result.into_iter().collect::<BTreeMap<_, _>>();
            log_value(&sorted_result);
        }
        Command::Ticker { pairs } => {
            let result = api.ticker(pairs).expect("api call failed");
            let sorted_result = result.into_iter().collect::<BTreeMap<_, _>>();
            log_value(&sorted_result);
        }
        Command::GetBalance => {
            let result = api.get_account_balance().expect("api call failed");
            let sorted_result = result.into_iter().collect::<BTreeMap<_, _>>();
            log_value(&sorted_result);
        }
        Command::GetTradeVolume { pairs } => {
            let result = api.get_trade_volume(pairs).expect("api call failed");
            log_value(&result);
        }
        Command::GetWebSocketsToken => {
            let result = api.get_websockets_token().expect("api call failed");
            log_value(&result);
        }
        Command::GetOpenOrders => {
            let result = api.get_open_orders(None).expect("api call failed");
            let sorted_result = result.open.into_iter().collect::<BTreeMap<_, _>>();
            log_value(&sorted_result);
        }
        Command::CancelOrder { id } => {
            let result = api.cancel_order(id).expect("api call failed");
            log_value(&result);
        }
        Command::CancelAllOrders => {
            let result = api.cancel_all_orders().expect("api call failed");
            log_value(&result);
        }
        Command::CancelAllOrdersAfter { timeout } => {
            let result = api.cancel_all_orders_after(timeout).expect("api call failed");
            log_value(&result);
        }
        Command::MarketBuy { volume, pair } => {
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
            log_value(&result);
        }
        Command::MarketSell { volume, pair } => {
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
            log_value(&result);
        }
        Command::LimitBuy { volume, pair, price } => {
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
            log_value(&result);
        }
        Command::LimitSell { volume, pair, price } => {
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
            log_value(&result);
        }
    }
}
