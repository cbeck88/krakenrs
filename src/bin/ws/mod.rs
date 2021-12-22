use ctrlc::set_handler;
use displaydoc::Display;
use env_logger::{fmt::Color, Builder, Env};
use krakenrs::{
    ws::{KrakenPrivateWsConfig, KrakenWsAPI, KrakenWsConfig},
    KrakenCredentials, KrakenRestAPI, KrakenRestConfig,
};
use log::Level;
use std::{
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
}

/// Commands supported by krak-feed executable
#[derive(StructOpt, Display)]
enum Command {
    /// Get websockets feed for one or more asset-pair books
    Book { pairs: Vec<String> },

    /// Get websockets feed for own orders
    OwnOrders {},
}

static PROCESS_TERMINATING: AtomicBool = AtomicBool::new(false);

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

    set_handler(|| PROCESS_TERMINATING.store(true, Ordering::SeqCst)).expect("could not set termination handler");

    match config.command {
        Command::Book { pairs } => {
            let ws_config = KrakenWsConfig {
                subscribe_book: pairs.clone(),
                book_depth: 10,
                private: None,
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
                        println!("");
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

                if PROCESS_TERMINATING.load(Ordering::SeqCst) {
                    log::debug!("Process terminating");
                    return;
                }
            }
        }
        Command::OwnOrders {} => {
            // First get a websockets token
            let mut kc_config = KrakenRestConfig::default();

            // Load credentials from disk if specified
            if let Some(creds) = config.creds {
                log::info!("Credentials path: {:?}", creds);
                kc_config.creds = KrakenCredentials::load_json_file(creds).expect("credential file error");
            }

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
            let api = KrakenWsAPI::new(ws_config).expect("could not connect to websockets api");

            let mut prev = api.get_open_orders();

            loop {
                let next = api.get_open_orders();

                if next != prev {
                    println!("Orders:");
                    println!("{}", serde_json::to_string_pretty(&next).unwrap());
                    println!("");
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
    };
}
