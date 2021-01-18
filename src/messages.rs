//! Structures representing json schema sent to and from Kraken

use serde::{Deserialize, Serialize};

/// Kraken responds to APIs with a json body consisting of "error:" and "result:" fields.
/// The error part is an array of strings encoded as:
/// <char-severity code><string-error category>:<string-error type>[:<string-extra info>]
/// The result part is a json object or array
#[derive(Debug, Serialize, Deserialize)]
pub struct KrakenResult<Result: Serialize> {
    /// Kraken API returns error strings in an array marked "error"
    pub error: Vec<String>,
    /// Kraken API returns results here, separated from error
    pub result: Result,
}

/// Empty json object (used as arguments for some APIs)
#[derive(Debug, Serialize, Deserialize)]
pub struct Empty {}

/// Result of kraken public "Time" API call
#[derive(Debug, Serialize, Deserialize)]
pub struct Time {
    unixtime: u64,
}

/// Result of kraken "SystemStatus" API call
#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    status: String,
    timestamp: String,
}
