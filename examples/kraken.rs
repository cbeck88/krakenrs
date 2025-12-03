use anstyle::{AnsiColor, Style};
use clap::{Parser, Subcommand};
use core::convert::TryFrom;
use core::fmt::Debug;
use displaydoc::Display;
use env_logger::{Builder, Env};
use krakenrs::{BsType, KrakenCredentials, KrakenRestAPI, KrakenRestConfig, LimitOrder, MarketOrder, OrderFlag};
use log::Level;
use rust_decimal::Decimal;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::PathBuf,
};

/// Structure representing parsed command-line arguments to "kraken" executable
#[derive(Parser)]
struct KrakenConfig {
    #[command(subcommand)]
    command: Command,

    /// Credentials file, formatted in json. Required only for private APIs
    creds: Option<PathBuf>,

    /// Whether to pass "validate = true" with any orders (for testing)
    #[arg(short, long)]
    validate: bool,
}

/// Commands supported by kraken executable
#[derive(Subcommand, Display)]
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
    /// Get OHLC data {pair}, {since:?}, @ {interval:?} minutes
    OHLC {
        pair: String,
        since: Option<String>,
        interval: Option<u16>,
    },
    /// Get recent trades since some timestamp: {pair}, {since:?}
    RecentTrades { pair: String, since: Option<String> },
    /// Get account balance
    GetBalance,
    /// Get account trade volume (and fees): {pairs:?}
    GetTradeVolume { pairs: Vec<String> },
    /// Get websockets token
    GetWebSocketsToken,
    /// Query specific orders by order id
    QueryOrders { order_ids: Vec<String> },
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
    /// Get deposit methods for asset: {asset}
    GetDepositMethods { asset: String },
    /// Get deposit addresses for asset and method: {asset} {method}
    GetDepositAddresses {
        asset: String,
        method: String,
        /// Generate a new address
        #[arg(long)]
        new: bool,
        /// Amount to deposit (required for Bitcoin Lightning)
        #[arg(long)]
        amount: Option<Decimal>,
    },
    /// Get withdrawal addresses
    GetWithdrawalAddresses {
        /// Optional asset to filter by
        #[arg(long)]
        asset: Option<String>,
        /// Optional method to filter by
        #[arg(long)]
        method: Option<String>,
    },
    /// Withdraw funds: {asset} {key} {amount}
    Withdraw {
        asset: String,
        /// Withdrawal key name (as configured in your Kraken account)
        key: String,
        amount: String,
        /// Optional address to verify against key
        #[arg(long)]
        address: Option<String>,
        /// Optional maximum fee
        #[arg(long)]
        max_fee: Option<String>,
    },
    /// Get status of recent withdrawals
    GetWithdrawStatus {
        /// Optional asset to filter by
        #[arg(long)]
        asset: Option<String>,
        /// Optional method to filter by
        #[arg(long)]
        method: Option<String>,
        /// Optional start timestamp (unix time) for filtering
        #[arg(long)]
        start: Option<String>,
        /// Optional end timestamp (unix time) for filtering
        #[arg(long)]
        end: Option<String>,
        /// Optional cursor for pagination
        #[arg(long)]
        cursor: Option<String>,
        /// Optional limit for number of results (default 500)
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Get status of recent deposits
    GetDepositStatus {
        /// Optional asset to filter by
        #[arg(long)]
        asset: Option<String>,
        /// Optional method to filter by
        #[arg(long)]
        method: Option<String>,
        /// Optional start timestamp (unix time) for filtering
        #[arg(long)]
        start: Option<String>,
        /// Optional end timestamp (unix time) for filtering
        #[arg(long)]
        end: Option<String>,
        /// Optional cursor for pagination
        #[arg(long)]
        cursor: Option<String>,
        /// Optional limit for number of results (default 500)
        #[arg(long)]
        limit: Option<u32>,
        /// Whether to include originators field in response
        #[arg(long)]
        originators: bool,
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
            let color = match record.level() {
                Level::Error => AnsiColor::Red,
                Level::Warn => AnsiColor::Yellow,
                Level::Info => AnsiColor::Green,
                Level::Debug => AnsiColor::Cyan,
                Level::Trace => AnsiColor::Magenta,
            };
            let style = Style::new().fg_color(Some(color.into())).bold();

            writeln!(
                buf,
                "{} {style}{}{style:#} [{} {}:{}] {}",
                chrono::Utc::now(),
                record.level(),
                record.module_path().unwrap_or("?"),
                record.file().unwrap_or("?"),
                record.line().unwrap_or(0),
                record.args(),
            )
        })
        .init();

    let config = KrakenConfig::parse();

    let mut builder = KrakenRestConfig::builder();

    // Load credentials from disk if specified
    if let Some(creds) = config.creds {
        log::info!("Credentials path: {:?}", creds);
        builder = builder.creds(KrakenCredentials::load_json_file(creds).expect("credential file error"));
    }

    let kc_config = builder.build().expect("error building config");

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
        Command::OHLC { pair, since, interval } => {
            let result = if let Some(interval) = interval {
                api.ohlc_at_interval(pair, interval, since)
            } else {
                api.ohlc(pair, since)
            }
            .expect("api call failed");
            log_value(&result);
        }
        Command::RecentTrades { pair, since } => {
            let result = api.get_recent_trades(pair, since).expect("api call failed");
            log_value(&result);
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
        Command::QueryOrders { order_ids } => {
            let result = api.query_orders(order_ids).expect("api call failed");
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
        Command::GetDepositMethods { asset } => {
            let result = api.get_deposit_methods(asset).expect("api call failed");
            log_value(&result);
        }
        Command::GetDepositAddresses {
            asset,
            method,
            new,
            amount,
        } => {
            use krakenrs::DepositAddressesRequest;
            let result = api
                .get_deposit_addresses(DepositAddressesRequest {
                    asset,
                    method,
                    new: if new { Some(true) } else { None },
                    amount,
                })
                .expect("api call failed");
            log_value(&result);
        }
        Command::GetWithdrawalAddresses { asset, method } => {
            let result = api.get_withdrawal_addresses(asset, method).expect("api call failed");
            log_value(&result);
        }
        Command::Withdraw {
            asset,
            key,
            amount,
            address,
            max_fee,
        } => {
            use krakenrs::WithdrawRequest;
            let result = api
                .withdraw(WithdrawRequest {
                    asset,
                    key,
                    amount,
                    address,
                    max_fee,
                })
                .expect("api call failed");
            log_value(&result);
        }
        Command::GetWithdrawStatus {
            asset,
            method,
            start,
            end,
            cursor,
            limit,
        } => {
            use krakenrs::WithdrawStatusRequest;
            let result = api
                .get_withdraw_status(WithdrawStatusRequest {
                    asset,
                    method,
                    start,
                    end,
                    cursor,
                    limit,
                })
                .expect("api call failed");
            log_value(&result);
        }
        Command::GetDepositStatus {
            asset,
            method,
            start,
            end,
            cursor,
            limit,
            originators,
        } => {
            use krakenrs::DepositStatusRequest;
            let result = api
                .get_deposit_status(DepositStatusRequest {
                    asset,
                    method,
                    start,
                    end,
                    cursor,
                    limit,
                    originators: if originators { Some(true) } else { None },
                })
                .expect("api call failed");
            log_value(&result);
        }
    }
}
