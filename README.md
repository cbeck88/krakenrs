krakenrs
========

A Kraken API client in Rust.

This library provides a Rust client object implementing many of the calls from the [Kraken REST API](https://docs.kraken.com/rest/)
with an idiomatic Rust interface, including getting ticker info and making market and limit orders.

- Requests and responses are strongly-typed, conversion done using [`serde_json`](https://docs.serde.rs/serde_json/)
- [`reqwest`](https://docs.rs/reqwest/0.11.0/reqwest/) is used for https
- [`RustCrypto`](https://docs.rs/hmac/0.10.1/hmac/) crates used for the Kraken authentication scheme
- Robust error handling

Both public and private APIs are supported, but not all the calls and options are exposed, only the ones that were needed.
If something you need is missing, patches are welcome! Just open a github issue or pull request.

Disclaimer
----------

Use at your own risk. If you build trading software using this component and you suffer a loss because of a bug, I am not responsible.

Running demo functionality
--------------------------

The `krak` binary target is a simple demo that can be used to exercise the functionality of `krakenrs`.
It is a command-line target that can parse a credentials file, connect to kraken and make a single
API call, and print the response.

Usage:
- Build everything: `cargo build`.
- Run `./target/debug/krak --help` for usage information.
  For example, you can see the trading system's current status with
  `./krak system-status`, or see asset pairs and current prices with
  `./krak asset-pairs`, `./krak ticker AAVEUSD`
- If you want to use private APIs, go to your Kraken account and create an API key.
  Then create a json file with your credentials, with the following schema:
  ```
  {
     "key": "ASDF",
     "secret: "jklw=="
  }
  ```
- Private APIs are invoked for example like:
  `./krak path/to/creds get-open-orders`
  `./krak path/to/creds --validate market-buy 0.02 AAVEUSD`

Other projects of interest
--------------------------

- [`coinnect`](https://github.com/hugues31/coinnect)
- [`python3-krakenex`](https://github.com/veox/python3-krakenex)
