use core::convert::TryFrom;
use core::fmt::Debug;
use displaydoc::Display;
use krakenrs::{
    BsType, KrakenClientConfig, KrakenCredentials, KrakenRestAPI, LimitOrder, MarketOrder,
    OrderFlag,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use structopt::StructOpt;

/// Structure representing parsed command-line arguments to krak executable
#[derive(StructOpt)]
struct KrakConfig {
    #[structopt(subcommand)]
    command: Command,

    /// Credentials file, formatted in json. Required only for private APIs
    #[structopt(parse(from_os_str))]
    creds: Option<PathBuf>,

    /// Whether to pass "validate = true" with any orders (for testing)
    #[structopt(short, long)]
    validate: bool,
}

/// Commands supported by krak executable
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
            println!("{}", pretty);
        }
        Err(err) => {
            eprintln!("Could not pretty-print structure: {:?}: {}", val, err);
        }
    }
}

fn main() {
    let config = KrakConfig::from_args();

    let mut kc_config = KrakenClientConfig::default();

    // Load credentials from disk if specified
    if let Some(creds) = config.creds {
        let current_dir = std::env::current_dir().expect("Could not get current directory");
        let path = current_dir.join(creds);
        eprintln!("Credentials path: {:?}", path);
        let creds_file =
            std::fs::read_to_string(path).expect("Could not read specified credentials file");
        let creds_data: KrakenCredentials =
            serde_json::from_str(&creds_file).expect("Could not parse credentials file as json");
        if creds_data.key.is_empty() {
            panic!("Missing credentials 'key' value");
        }
        if creds_data.secret.is_empty() {
            panic!("Missing credentials 'secret' value");
        }
        kc_config.creds = creds_data;
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
            let result = api
                .cancel_all_orders_after(timeout)
                .expect("api call failed");
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
        Command::LimitBuy {
            volume,
            pair,
            price,
        } => {
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
        Command::LimitSell {
            volume,
            pair,
            price,
        } => {
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
