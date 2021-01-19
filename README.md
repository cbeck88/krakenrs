krakenrs
========

A (WIP) Kraken API client in Rust.

- Requests and responses are strongly-typed, conversion done using [`serde_json`](https://docs.serde.rs/serde_json/)
- [`reqwest`](https://docs.rs/reqwest/0.11.0/reqwest/) is used for https
- [`RustCrypto`](https://docs.rs/hmac/0.10.1/hmac/) crates used for the Kraken authentication scheme
- Robust error handling

Public and private APIs are supported, but not all the options are exposed.
If something you need is missing, pull requests are welcome!
