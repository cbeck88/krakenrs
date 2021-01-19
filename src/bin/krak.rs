use core::convert::TryFrom;
use displaydoc::Display;
use krakenrs::{KrakenAPI, KrakenClientConfig, KrakenResult};
use serde::Serialize;
use structopt::StructOpt;

/// Structure representing parsed command-line arguments to krak executable
#[derive(StructOpt)]
struct KrakConfig {
    #[structopt(subcommand)]
    command: Command,
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
}

/// Take the "error" field from KrakenResult and log errors on stderr
/// Then, discard them, returning only the parsed result.
fn log_errors<T: Serialize>(kraken_result: KrakenResult<T>) -> T {
    for err in kraken_result.error {
        eprintln!("{}", err);
    }
    kraken_result.result
}

fn main() {
    let config = KrakConfig::from_args();

    let mut api = KrakenAPI::try_from(KrakenClientConfig::default()).expect("could not create api");

    match config.command {
        Command::Time => {
            let result = log_errors(api.time().expect("api call failed"));
            println!("{:?}", result);
        }
        Command::SystemStatus => {
            let result = log_errors(api.system_status().expect("api call failed"));
            println!("{:?}", result);
        }
        Command::Assets => {
            let result = log_errors(api.assets().expect("api call failed"));
            println!("{:?}", result);
        }
    }
}
