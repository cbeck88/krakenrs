use core::convert::TryFrom;
use krakenrs::{KrakenAPI, KrakenClientConfig, KrakenResult, SystemStatus, Time};
use serde::Serialize;
use structopt::StructOpt;

#[derive(StructOpt)]
struct KrakConfig {
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt)]
enum Command {
    Time,
    SystemStatus,
}

fn log_errors<T: Serialize>(kraken_result: KrakenResult<T>) -> T {
    for err in kraken_result.error {
        eprintln!("{}", err);
    }
    eprintln!(
        "{}",
        serde_json::to_string(&kraken_result.result).expect("Could not serialize json")
    );
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
