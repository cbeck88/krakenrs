//! Serde helper modules for custom serialization/deserialization patterns.
//!
//! # Background
//!
//! Previously this crate used `serde_with` for these helpers:
//! - `#[serde(with = "serde_with::rust::StringWithSeparator::<CommaSeparator>")]` for comma-separated sets
//! - `#[serde(with = "serde_with::rust::display_fromstr")]` for Display/FromStr types
//! - `#[serde(deserialize_with = "serde_with::rust::default_on_error::deserialize")]` for fallible deserialization
//!
//! However:
//! - `serde_with` 1.x brings in very old dependencies (darling 0.13, strsim 0.10)
//! - `serde_with` 3.x has a significantly different API that doesn't work correctly with
//!   our custom types that implement `Display`/`FromStr` (the `StringWithSeparator` type
//!   changed to require different trait bounds)
//!
//! These simple helper modules provide the same functionality without the dependency baggage.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};
use std::collections::BTreeSet;
use std::fmt::Display;
use std::str::FromStr;

/// Serialize/deserialize a `BTreeSet<T>` as a comma-separated string.
///
/// Requires `T: Display + FromStr + Ord`.
///
/// # Example
/// ```ignore
/// #[serde(with = "crate::serde_helpers::comma_separated")]
/// pub flags: BTreeSet<MyFlag>,
/// ```
pub mod comma_separated {
    use super::*;

    pub fn serialize<T, S>(set: &BTreeSet<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        let s: String = set.iter().map(|item| item.to_string()).collect::<Vec<_>>().join(",");
        s.serialize(serializer)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<BTreeSet<T>, D::Error>
    where
        T: FromStr + Ord,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        if s.is_empty() {
            return Ok(BTreeSet::new());
        }
        s.split(',')
            .map(|item| {
                item.parse::<T>()
                    .map_err(|e| D::Error::custom(format!("failed to parse: {}", e)))
            })
            .collect()
    }
}

/// Serialize/deserialize a type using its `Display` and `FromStr` implementations.
///
/// # Example
/// ```ignore
/// #[serde(with = "crate::serde_helpers::display_fromstr")]
/// pub validate: bool,
/// ```
pub mod display_fromstr {
    use super::*;

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse::<T>()
            .map_err(|e| D::Error::custom(format!("failed to parse: {}", e)))
    }
}

/// Deserialize a value, returning `None` if deserialization fails.
///
/// Only provides `deserialize` - serialization uses the default behavior.
///
/// # Example
/// ```ignore
/// #[serde(deserialize_with = "crate::serde_helpers::default_on_error::deserialize")]
/// pub leverage: Option<Decimal>,
/// ```
pub mod default_on_error {
    use super::*;

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        // Try to deserialize, but if it fails, return None
        Ok(Option::<T>::deserialize(deserializer).unwrap_or(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::fmt;

    // Test type for comma_separated and display_fromstr
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    enum TestFlag {
        Alpha,
        Beta,
        Gamma,
    }

    impl fmt::Display for TestFlag {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                TestFlag::Alpha => write!(f, "alpha"),
                TestFlag::Beta => write!(f, "beta"),
                TestFlag::Gamma => write!(f, "gamma"),
            }
        }
    }

    impl FromStr for TestFlag {
        type Err = String;
        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s {
                "alpha" => Ok(TestFlag::Alpha),
                "beta" => Ok(TestFlag::Beta),
                "gamma" => Ok(TestFlag::Gamma),
                _ => Err(format!("unknown flag: {}", s)),
            }
        }
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestCommaSeparated {
        #[serde(with = "comma_separated")]
        flags: BTreeSet<TestFlag>,
    }

    #[test]
    fn test_comma_separated_serialize() {
        let mut flags = BTreeSet::new();
        flags.insert(TestFlag::Alpha);
        flags.insert(TestFlag::Gamma);
        let test = TestCommaSeparated { flags };

        let json = serde_json::to_string(&test).unwrap();
        // BTreeSet maintains order, so Alpha comes before Gamma
        assert_eq!(json, r#"{"flags":"alpha,gamma"}"#);
    }

    #[test]
    fn test_comma_separated_deserialize() {
        let json = r#"{"flags":"beta,alpha,gamma"}"#;
        let test: TestCommaSeparated = serde_json::from_str(json).unwrap();

        let mut expected = BTreeSet::new();
        expected.insert(TestFlag::Alpha);
        expected.insert(TestFlag::Beta);
        expected.insert(TestFlag::Gamma);
        assert_eq!(test.flags, expected);
    }

    #[test]
    fn test_comma_separated_empty() {
        let json = r#"{"flags":""}"#;
        let test: TestCommaSeparated = serde_json::from_str(json).unwrap();
        assert!(test.flags.is_empty());
    }

    #[test]
    fn test_comma_separated_single() {
        let json = r#"{"flags":"beta"}"#;
        let test: TestCommaSeparated = serde_json::from_str(json).unwrap();

        let mut expected = BTreeSet::new();
        expected.insert(TestFlag::Beta);
        assert_eq!(test.flags, expected);
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestDisplayFromStr {
        #[serde(with = "display_fromstr")]
        value: bool,
    }

    #[test]
    fn test_display_fromstr_serialize() {
        let test = TestDisplayFromStr { value: true };
        let json = serde_json::to_string(&test).unwrap();
        assert_eq!(json, r#"{"value":"true"}"#);
    }

    #[test]
    fn test_display_fromstr_deserialize() {
        let json = r#"{"value":"false"}"#;
        let test: TestDisplayFromStr = serde_json::from_str(json).unwrap();
        assert!(!test.value);
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestDefaultOnError {
        #[serde(deserialize_with = "default_on_error::deserialize")]
        #[serde(default)]
        value: Option<i32>,
    }

    #[test]
    fn test_default_on_error_valid() {
        let json = r#"{"value":42}"#;
        let test: TestDefaultOnError = serde_json::from_str(json).unwrap();
        assert_eq!(test.value, Some(42));
    }

    #[test]
    fn test_default_on_error_null() {
        let json = r#"{"value":null}"#;
        let test: TestDefaultOnError = serde_json::from_str(json).unwrap();
        assert_eq!(test.value, None);
    }

    #[test]
    fn test_default_on_error_invalid() {
        // "not a number" can't be parsed as i32, should return None
        let json = r#"{"value":"not a number"}"#;
        let test: TestDefaultOnError = serde_json::from_str(json).unwrap();
        assert_eq!(test.value, None);
    }
}
