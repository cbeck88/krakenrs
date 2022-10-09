use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// test_ohlc_serialization sets up dummy data and then reads it in, serializing it to the OHLCResponse.
    ///   A working demonstration of this technique can be found below
    #[test]
    fn test_ohlc_serialization() {
        let ohlc_test_data_from_result = r#"
        {
          "LOLCOIN": [
            [
                1616662740,
                "52591.9",
                "52599.9",
                "52591.8",
                "52599.9",
                "52599.1",
                "0.11091626",
                5
                ]
            ],
            "last": 1616662740
        }
        "#;

        let ohlc_info: AssetOHLCInfo = serde_json::from_str(ohlc_test_data_from_result).unwrap();
        assert_eq!(ohlc_info.last, 1616662740);
    }

    /// It seems that when the result for AssetTickerInfo is unwrapped and serialized,
    ///   it's scope is smaller than the previous example.
    #[test]
    fn test_asset_ticker_serialization() {
        let ati_test_data = r#"
        {
            "a": [
                "52609.60000",
                "1",
                "1.000"
            ],
            "b": [
                "52609.50000",
                "1",
                "1.000"
            ],
            "c": [
                "52641.10000",
                "0.00080000"
            ],
            "v": [
                "1920.83610601",
                "7954.00219674"
            ],
            "p": [
                "52389.94668",
                "54022.90683"
            ],
            "t": [
                23329,
                80463
            ],
            "l": [
                "51513.90000",
                "51513.90000"
            ],
            "h": [
                "53219.90000",
                "57200.00000"
            ],
            "o": "52280.40000"
        }
        "#;

        let ati: AssetTickerInfo = serde_json::from_str(ati_test_data).unwrap();
        assert_eq!(ati.a[0], "52609.60000");
    }

    #[test]
    fn test_client_connection() {
      let conf = KrakenRestConfig::default();
      let client = KrakenRestAPI::try_from(conf).unwrap();

      let ohlc_res = client.ohlc(vec!["XXBTZUSD".to_string()]).unwrap();
      for candle in ohlc_res.pair {
          println!("Candle: {:?}", candle);
      }

      // let xxbtzusd = _ohlc_res.get("XXBTZUSD").unwrap();
      // assert!(xxbtzusd.pair.0 == 0);
    }
}
