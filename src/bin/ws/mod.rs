use ctrlc::set_handler;
use displaydoc::Display;
use krakenrs::{KrakenWsAPI, KrakenWsConfig};
use std::{
    collections::BTreeMap,
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
    #[allow(unused)]
    #[structopt(parse(from_os_str))]
    creds: Option<PathBuf>,
}

/// Commands supported by krak-feed executable
#[derive(StructOpt, Display)]
enum Command {
    /// Get websockets feed for one or more asset-pair books
    Book { pairs: Vec<String> },
}

static PROCESS_TERMINATING: AtomicBool = AtomicBool::new(false);

pub fn main() {
    let config = KrakFeedConfig::from_args();

    set_handler(|| PROCESS_TERMINATING.store(true, Ordering::SeqCst))
        .expect("could not set termination handler");

    match config.command {
        Command::Book { pairs } => {
            let ws_config = KrakenWsConfig {
                subscribe_book: pairs.clone(),
                book_depth: 10,
            };
            let api = KrakenWsAPI::new(ws_config).expect("could not connect to websockets api");

            let mut prev = pairs
                .iter()
                .map(|pair| (pair.clone(), api.get_book(pair)))
                .collect::<BTreeMap<_, _>>();

            loop {
                let next = pairs
                    .iter()
                    .map(|pair| (pair.clone(), api.get_book(pair)))
                    .collect::<BTreeMap<_, _>>();

                if next != prev {
                    for (pair, book_data) in &next {
                        println!("{} asks:", pair);
                        for (price, entry) in book_data.ask.iter() {
                            println!("{}\t\t{}", price, entry.volume);
                        }
                        println!("{} bids:", pair);
                        for (price, entry) in book_data.bid.iter().rev() {
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
                    println!("Stream closed");
                    return;
                }

                if PROCESS_TERMINATING.load(Ordering::SeqCst) {
                    eprintln!("Process terminating");
                    return;
                }
            }
        }
    };
}
