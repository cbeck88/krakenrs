krakenrs
========

Unofficial bindings to Kraken spot trading API in Rust.

[![Crates.io](https://img.shields.io/crates/v/conf?style=flat-square)](https://crates.io/crates/krakenrs)
[![Crates.io](https://img.shields.io/crates/d/conf?style=flat-square)](https://crates.io/crates/krakenrs)
[![License](https://img.shields.io/badge/license-WTFPL%202.0-blue?style=flat-square)](LICENSE-WTFPL)

[API Docs](https://docs.rs/krakenrs/latest/krakenrs/) | [Examples](./examples)

This library provides a Rust client object implementing many of the calls from the [Kraken REST API](https://docs.kraken.com/rest/)
with an idiomatic Rust interface, including getting ticker info and making market and limit orders. Additionally it provides access
to the [Kraken WS API](https://docs.kraken.com/ws/), both subscribing to feeds and submitting orders.

- Requests and responses are strongly-typed, conversion done using [`serde_json`](https://docs.serde.rs/serde_json/)
- [`reqwest`](https://docs.rs/reqwest/0.11.0/reqwest/) is used for https
- [`tokio-tungstenite`](https://docs.rs/tokio-tungstenite/latest/tokio_tungstenite/) is used for websockets
- [`RustCrypto`](https://docs.rs/hmac/0.10.1/hmac/) crates used for the Kraken authentication scheme
- [`rust_decimal`](https://docs.rs/rust_decimal/latest/rust_decimal/) used to represent Decimal values from kraken
- [`log`](https://docs.rs/log/latest/log/) is used for logging
- Robust error handling

Both public and private APIs are supported, but not all the calls and options are exposed, only the ones that were needed.
If something you need is missing, patches are welcome! Just open a github issue or pull request.

Features
--------

To get the websockets API, the `"ws"` feature must be enabled. It is on by default.
Otherwise you only get the REST API, which can do all the same things (and more), but has more strict rate limits.

We only support Kraken's websockets v1 API right now. In the future we might add support for the websockets v2 API. We don't plan to deprecate the websockets v1 API bindings anytime soon -- they still work great.

As of version 6, `serde_json/arbitrary_precision` feature is required for the crate to work, because some parts of the REST API and the websockets v1 API represent unix timestamps as json numbers. This may have some performance impact for other parts of your project, because the json parser will make more string allocations. But in most cases it shouldn't be a big deal. If this is a problem for your project, what I suggest is to stick to version 5 if possible. Otherwise, we could contemplate using feature flagging to remove those library features that would break if `arbitrary_precision` is off, or support using an alternative json implementation to `serde_json`. Please open a github issue if you want to discuss and contribute to this.

Threading
---------

Unlike some other bindings, these are not async APIs (although the websockets feeds are implicitly asynchronous).

We have chosen to create blocking APIs for the Kraken REST API version for a few reasons:
* simplicity
* ease of debugging (backtraces and flamegraphs don't lie)
* when trying to make multiple private REST API calls in parallel, we often see invalid nonce errors.
  This is because the nonces are based on timestamps, but when multiple requests are created and sent
  in parallel, this is inherently racy and sometimes the request with the higher nonce will be processed
  by kraken first, invalidating the others.

Additionally, the REST API has quite strict rate limits so making large numbers of requests
in parallel isn't really possible.

Instead, it seems better to lean on the Websockets API, which is easy to use whether you want to use
an async runtime or not, and not make lots of calls to the REST API.

If you are using an async runtime like tokio, you can avoid blocking the executor by wrapping sequences of calls to
the REST API with `task::spawn_blocking` or similar, or just do all of your work with `krakenrs` on a blocking thread
and use channels etc. to pass data around.

Examples
--------

Here are some short examples, besides the `cargo` examples:

REST API:

```rs
use krakenrs::{KrakenRestAPI, KrakenRestConfig};
use serde_json::to_string_pretty;

fn main() {
    let kc_config = KrakenRestConfig::default();
    let api = KrakenRestAPI::try_from(kc_config).expect("could not create kraken api");

    println!(
        "{}",
        to_string_pretty(
            &api.asset_pairs(vec!["XBTUSD".to_string(), "SOLBTC".to_string()])
                .expect("api call failed")
        )
        .unwrap()
    );

    println!(
        "{}",
        to_string_pretty(
            &api.ticker(vec!["XBTUSD".to_string()])
                .expect("api call failed")
        )
        .unwrap()
    );
}
```

Websockets API:

```rs
use krakenrs::ws::{KrakenWsConfig, KrakenWsAPI};
use std::{
    time::Duration,
    thread,
};

fn main() {
    let pairs = vec!["USD/CAD".to_string()];

    let ws_config = KrakenWsConfig::builder()
        .subscribe_book(pairs)
        .build()
        .unwrap();
    let api = KrakenWsAPI::new(ws_config).expect("could not connect to websockets api");

    loop {
        thread::sleep(Duration::from_millis(500));
        let books = api.get_all_books();

        for (pair, book) in books {
            println!("{}", pair);
            println!("{} bids:", pair);
            for (price, entry) in book.bid.iter() {
                println!("{}\t\t{}", price, entry.volume);
            }
            println!("{} asks:", pair);
            for (price, entry) in book.ask.iter() {
                println!("{}\t\t{}", price, entry.volume);
            }
            println!();
            if book.checksum_failed {
                println!("Checksum failed, aborting");
                return;
            }
        }
        if api.stream_closed() { return; }
    }
}
```

The `KrakenWsAPI` object spawns a worker thread internally which drives the websockets connection.
If you don't want that you can import the `KrakenWsClient` object instead and arrange the worker
thread as you like, while observing latest feed data in other threads using the handle to the `ApiResult` object.

Disclaimer
----------

Use at your own risk. If you build trading software using this component and you suffer a loss because of a bug, I am not responsible.

Rest API Demo
-------------

The `kraken` example target is a simple demo that can be used to exercise the rest API functionality.
It is a command-line target that can parse a credentials file, connect to kraken and make a single
API call, and print the response.

Usage:
- Run `cargo run --example kraken -- --help` for usage information.
  For example, you can see the trading system's current status with
  `cargo run --example kraken -- system-status`, or see asset pairs and current prices with
  `cargo run --example kraken -- asset-pairs`,
  `cargo run --example kraken -- ticker AAVEUSD`
- If you want to use private APIs, go to your Kraken account and create an API key.
  Then create a json file with your credentials, with the following schema:
  ```
  {
     "key": "ASDF",
     "secret: "jklw=="
  }
  ```
- Private APIs are invoked for example like:
  `cargo run --example kraken path/to/creds get-open-orders`
  `cargo run --example kraken path/to/creds --validate market-buy 0.02 AAVEUSD`

Websockets Feed Demo
--------------------

The `kraken-feed` example target can subscribe to, and print the results of, websockets feeds.

Usage:
- Run `cargo run --example kraken-feed --help` for usage information.
- `cargo run --example kraken-feed book XBT/USD` will display the bitcoin/USD order book continuously.
- `cargo run --example kraken-feed trades XBT/USD` will display the bitcoin/USD trade sequence continuously.

Note that you have to use the "websockets name" of an asset pair when using the websockets APIs. This is the
field `wsname` in the asset-pairs list.

Other projects of interest
--------------------------

- [`dovahcrow kraken rust client`](https://github.com/dovahcrow/kraken-rs)
- [`async kraken ws`](https://crates.io/crates/async_kraken_ws)
- [`coinnect`](https://github.com/hugues31/coinnect)
- [`python3-krakenex`](https://github.com/veox/python3-krakenex)
- [`kraken_ws_orderbook`](https://github.com/jurijbajzelj/kraken_ws_orderbook)

Reference
---------

- [Kraken REST API Docs](https://docs.kraken.com/rest/)
- [Kraken WS API Docs](https://docs.kraken.com/websockets/#overview)
