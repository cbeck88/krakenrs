krakenrs
========

A (WIP) Kraken API client in Rust.

- Requests and responses are strongly-typed, conversion done using [`serde_json`](https://docs.serde.rs/serde_json/)
- [`reqwest`](https://docs.rs/reqwest/0.11.0/reqwest/) is used for https
- [`RustCrypto`](https://docs.rs/hmac/0.10.1/hmac/) crates used for the Kraken authentication scheme
- Robust error handling

Public and private APIs are supported, but not all the options are exposed.
If something you need is missing, pull requests are welcome!

Running demo functionality
--------------------------

The `krak` binary target is a simple demo that can be used to exercise the functionality of `krakenrs`.
It is a command-line target that can parse a credentials file, connect to kraken and make a single
API call, and print the response.

Usage:
- Build everything: `cargo build`.
- Run `./target/debug/krak --help` for usage information.
  For example, you can see the trading system's current status with
  `./target/debug/krak system-status`.
- If you want to use private APIs, go to your Kraken account and create an API key.
  Then create a json file with your credentials, with the following schema:
  ```
  {
     "key": "ASDF",
     "secret: "jklw=="
  }
  ```
- Private APIs are invoked for example like:
  `./target/debug/krak path/to/creds get-open-orders`

Other projects of interest
--------------------------

- [`coinnect`](https://github.com/hugues31/coinnect)
- [`python3-krakenex`](https://github.com/veox/python3-krakenex)
