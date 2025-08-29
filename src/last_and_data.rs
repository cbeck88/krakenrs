use core::marker::PhantomData;
use serde::{
    de::{Error, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};

/// The LastAndData struct is a helper to deal with wierdness in some of the
/// Kraken REST API schemas.
///
/// * OHLC candle (see issue #3)
/// * Get Recent Trades
///
/// These endpoints return a json object with two members:
/// * one page of data is returned at a dynamically-changing (but uninteresting) key,
/// * "last" field mapping to a number (which is either a json number or string...),
///   which can be used to find the next page
///
/// Idiomatically, we'd like to make this key static, so that it's easier to get
/// to the data in rust, and an extra layer of hashmap etc. is not needed.
///
/// LastAndData<T> has a custom deserialize implementation that accomplishes this,
/// based on comments in issue #3.
///
/// This dynamic key simply becomes "data" after deserializing this way.
#[derive(Clone, Debug, Default, Serialize)]
pub struct LastAndData<T> {
    /// Used to find the next page of data
    pub last: String,
    /// Data associated to this response.
    pub data: T,
}

impl<'de, T> Deserialize<'de> for LastAndData<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(LastAndDataVisitor {
            _data: Default::default(),
        })
    }
}

struct LastAndDataVisitor<T> {
    _data: PhantomData<T>,
}

impl<'de, T> Visitor<'de> for LastAndDataVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = LastAndData<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "a map containing a 'last' counter, and a pair-name keying to data"
        )
    }

    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        const ERR_MSG: &str = "Expected map with two keys: last, and an asset pair";

        let key1 = map.next_key::<&str>()?.ok_or(M::Error::custom(ERR_MSG))?;

        if key1 == "last" {
            // last appeared first
            let last: StringOrNumber = map.next_value()?;
            let last = last.0;

            // The next key should be for the data
            let _key2 = map.next_key::<&str>()?.ok_or(M::Error::custom(ERR_MSG))?;
            let data = map.next_value()?;

            if map.next_key::<&str>()?.is_some() {
                return Err(M::Error::custom(ERR_MSG));
            }

            Ok(LastAndData { last, data })
        } else {
            // the data appeared first
            let data = map.next_value()?;

            // The next key should be last
            let key2 = map.next_key::<&str>()?.ok_or(M::Error::custom(ERR_MSG))?;
            if key2 != "last" {
                return Err(M::Error::custom(ERR_MSG));
            }
            let last: StringOrNumber = map.next_value()?;
            let last = last.0;

            if map.next_key::<&str>()?.is_some() {
                return Err(M::Error::custom(ERR_MSG));
            }

            Ok(LastAndData { last, data })
        }
    }
}

// Helper for allowing last to be a string or an integer
struct StringOrNumber(String);

struct StringOrNumberVisitor;

impl<'de> Deserialize<'de> for StringOrNumber {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StringOrNumberVisitor)
    }
}

impl<'de> Visitor<'de> for StringOrNumberVisitor {
    type Value = StringOrNumber;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "an integer, or a string-formatted integer")
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(StringOrNumber(v.to_string()))
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(StringOrNumber(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_last_and_data() {
        let text = r#"{"X":19,"last":"1756443816201051892"}"#;

        let obj: LastAndData<u64> = serde_json::from_str(text).unwrap();

        assert_eq!(obj.data, 19);
        assert_eq!(obj.last, "1756443816201051892");
    }

    #[test]
    fn test_last_and_data_int() {
        let text = r#"{"X":19,"last":1756443816201051892}"#;

        let obj: LastAndData<u64> = serde_json::from_str(text).unwrap();

        assert_eq!(obj.data, 19);
        assert_eq!(obj.last, "1756443816201051892");
    }

    #[test]
    fn test_last_and_data_vec() {
        let text = r#"{"AAVEUSD":[19, 27, 32],"last":"1756443816201051892"}"#;

        let obj: LastAndData<Vec<u64>> = serde_json::from_str(text).unwrap();

        assert_eq!(obj.data.len(), 3);
        assert_eq!(obj.data[0], 19);
        assert_eq!(obj.data[1], 27);
        assert_eq!(obj.data[2], 32);
        assert_eq!(obj.last, "1756443816201051892");
    }
}
