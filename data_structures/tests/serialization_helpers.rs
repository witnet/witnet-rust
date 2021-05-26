use serde::Serialize;
use witnet_data_structures::utxo_pool::UtxoSelectionStrategy;

fn test_json_serialization<T>(value: T, json_str: &'static str)
where
    T: Serialize,
    T: serde::de::DeserializeOwned,
    T: std::fmt::Debug,
    T: PartialEq,
{
    let x = value;
    let res = serde_json::to_string(&x).unwrap();

    assert_eq!(&res, json_str);

    let d: T = serde_json::from_str(&res).unwrap();
    assert_eq!(d, x);
}

#[test]
fn serialize_utxo_selection_strategy_no_from() {
    test_json_serialization(UtxoSelectionStrategy::Random { from: None }, r#""random""#);
    test_json_serialization(
        UtxoSelectionStrategy::BigFirst { from: None },
        r#""big_first""#,
    );
    test_json_serialization(
        UtxoSelectionStrategy::SmallFirst { from: None },
        r#""small_first""#,
    );
}
#[test]
fn serialize_utxo_selection_strategy_with_from() {
    // Address with all zeros for testing
    let my_pkh = "wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4"
        .parse()
        .unwrap();

    test_json_serialization(
        UtxoSelectionStrategy::Random { from: Some(my_pkh) },
        r#"{"strategy":"random","from":"wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4"}"#,
    );
    test_json_serialization(
        UtxoSelectionStrategy::BigFirst { from: Some(my_pkh) },
        r#"{"strategy":"big_first","from":"wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4"}"#,
    );
    test_json_serialization(
        UtxoSelectionStrategy::SmallFirst { from: Some(my_pkh) },
        r#"{"strategy":"small_first","from":"wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4"}"#,
    );
}

#[test]
fn deserialize_utxo_selection_strategy_object_no_from() {
    // Check that the "from" field is optional and defaults to None
    let d: UtxoSelectionStrategy = serde_json::from_str(r#"{"strategy": "random"}"#).unwrap();
    assert_eq!(d, UtxoSelectionStrategy::Random { from: None });
}

#[test]
fn deserialize_utxo_selection_strategy_object_invalid_from() {
    // Check that when the "from" field is invalid, the error message is easy to understand
    let d: Result<UtxoSelectionStrategy, _> =
        serde_json::from_str(r#"{"strategy": "Random", "from": "potato"}"#);
    assert_eq!(
        d.unwrap_err().to_string(),
        "Failed to deserialize Bech32: invalid length at line 1 column 40"
    );
}

#[test]
fn deserialize_utxo_selection_strategy_name_alias() {
    // Check that "random" and "Random" are both valid names
    let d: UtxoSelectionStrategy = serde_json::from_str(r#"{"strategy": "random"}"#).unwrap();
    assert_eq!(d, UtxoSelectionStrategy::Random { from: None });
    let d: UtxoSelectionStrategy = serde_json::from_str(r#"{"strategy": "Random"}"#).unwrap();
    assert_eq!(d, UtxoSelectionStrategy::Random { from: None });
    let d: UtxoSelectionStrategy = serde_json::from_str(r#""random""#).unwrap();
    assert_eq!(d, UtxoSelectionStrategy::Random { from: None });
    let d: UtxoSelectionStrategy = serde_json::from_str(r#""Random""#).unwrap();
    assert_eq!(d, UtxoSelectionStrategy::Random { from: None });
}
