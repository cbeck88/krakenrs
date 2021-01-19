use core::convert::TryFrom;
use krakenrs::{KrakenAPI, KrakenClientConfig, KrakenResult, SystemStatus, Time};
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
            let result: Time = log_errors(api.time().expect("api call failed"));
            println!("{:?}", result);
        }
        Command::SystemStatus => {
            let result: SystemStatus = log_errors(api.system_status().expect("api call failed"));
            println!("{:?}", result);
        }
    }
}
