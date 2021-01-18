krakenrs
========

A (WIP) Kraken API client in Rust.

- Requests and responses are strongly-typed, conversion done using `serde_json`
- `reqwest` is used for https
- `RustCrypto` crates used for the Kraken authentication scheme
- Robust error handling

Public and private APIs are supported, but not all the options are exposed.
If something you need is missing, pull requests are welcome!
