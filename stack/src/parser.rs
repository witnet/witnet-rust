use crate::{MyOperator, MyValue};
use scriptful::prelude::Item;
use std::str::FromStr;

pub fn script_to_string(script: &[Item<MyOperator, MyValue>]) -> String {
    let mut s = String::new();

    for item in script {
        s.push_str(&item_to_string(item));
        s.push('\n');
    }

    s
}

pub fn item_to_string(item: &Item<MyOperator, MyValue>) -> String {
    match item {
        Item::Operator(op) => operator_to_string(op).to_owned(),
        Item::Value(v) => value_to_string(v),
    }
}

pub fn operator_to_string(op: &MyOperator) -> &'static str {
    match op {
        MyOperator::Equal => "equal",
        MyOperator::Hash160 => "hash160",
        MyOperator::Sha256 => "sha256",
        MyOperator::CheckSig => "checksig",
        MyOperator::CheckMultiSig => "checkmultisig",
        MyOperator::CheckTimeLock => "checktimelock",
        MyOperator::Verify => "verify",
        MyOperator::If => "if",
        MyOperator::Else => "else",
        MyOperator::EndIf => "endif",
    }
}

pub fn value_to_string(v: &MyValue) -> String {
    match v {
        MyValue::Boolean(b) => b.to_string(),
        MyValue::Integer(i) => i.to_string(),
        MyValue::Bytes(x) => format!("0x{}", hex::encode(x)),
    }
}

#[derive(Debug)]
pub enum ScriptParseError {
    InvalidItem,
    InvalidHexBytes,
}

/// Parse script from a human-readable format.
///
/// Syntax:
/// * Different items must be separated by whitespace.
/// * Operators can be used by their name in lowercase: checksig sha256 endif
/// * Boolean values can be used by their name in lowercase: true false
/// * Integer values are just the integer: 2 30 -4
/// * Bytes values are encoded in hex and prefixed with 0x: 0x00 0x1122
pub fn parse_script(s: &str) -> Result<Vec<Item<MyOperator, MyValue>>, ScriptParseError> {
    let mut script = vec![];

    for word in s.split_whitespace() {
        script.push(parse_item(word)?);
    }

    Ok(script)
}

pub fn parse_item(s: &str) -> Result<Item<MyOperator, MyValue>, ScriptParseError> {
    let op = parse_operator(s);

    if let Some(op) = op {
        return Ok(Item::Operator(op));
    }

    let val = parse_value(s)?;

    if let Some(val) = val {
        return Ok(Item::Value(val));
    }

    Err(ScriptParseError::InvalidItem)
}

pub fn parse_boolean(s: &str) -> Option<MyValue> {
    bool::from_str(s).ok().map(MyValue::Boolean)
}

pub fn parse_integer(s: &str) -> Option<MyValue> {
    i128::from_str(s).ok().map(MyValue::Integer)
}

pub fn parse_bytes(s: &str) -> Result<Option<MyValue>, ScriptParseError> {
    if let Some(hex_str) = s.strip_prefix("0x") {
        let bytes = hex::decode(hex_str).map_err(|_e| ScriptParseError::InvalidHexBytes)?;
        return Ok(Some(MyValue::Bytes(bytes)));
    }

    Ok(None)
}

pub fn parse_value(s: &str) -> Result<Option<MyValue>, ScriptParseError> {
    let val = parse_boolean(s);

    if let Some(val) = val {
        return Ok(Some(val));
    }

    let val = parse_integer(s);

    if let Some(val) = val {
        return Ok(Some(val));
    }

    let val = parse_bytes(s)?;

    if let Some(val) = val {
        return Ok(Some(val));
    }

    Ok(None)
}

pub fn parse_operator(s: &str) -> Option<MyOperator> {
    let op = match s {
        "equal" => MyOperator::Equal,
        "hash160" => MyOperator::Hash160,
        "sha256" => MyOperator::Sha256,
        "checksig" => MyOperator::CheckSig,
        "checkmultisig" => MyOperator::CheckMultiSig,
        "checktimelock" => MyOperator::CheckTimeLock,
        "verify" => MyOperator::Verify,
        "if" => MyOperator::If,
        "else" => MyOperator::Else,
        "endif" => MyOperator::EndIf,
        _ => return None,
    };

    Some(op)
}

#[cfg(test)]
mod tests {
    use super::*;
    use witnet_data_structures::chain::PublicKey;

    #[test]
    fn script_to_string_multisig() {
        let pk_1 = PublicKey::from_bytes([1; 33]);
        let pk_2 = PublicKey::from_bytes([2; 33]);
        let pk_3 = PublicKey::from_bytes([3; 33]);

        let redeem_script = &[
            Item::Value(MyValue::Integer(2)),
            Item::Value(MyValue::Bytes(pk_1.pkh().bytes().to_vec())),
            Item::Value(MyValue::Bytes(pk_2.pkh().bytes().to_vec())),
            Item::Value(MyValue::Bytes(pk_3.pkh().bytes().to_vec())),
            Item::Value(MyValue::Integer(3)),
            Item::Operator(MyOperator::CheckMultiSig),
        ];

        let expected_script_string = "\
2
0xce041765675ad4d93378e20bd3a7d0d97ddcf338
0x7f2f54ff94459f3ac4d19d3219ce6ef06868eb8c
0xc2055e4b533b897450a2f7abc14a36882d4a2a10
3
checkmultisig
";

        assert_eq!(script_to_string(redeem_script), expected_script_string);
    }

    #[test]
    fn parse_string_to_script_multisig() {
        let script_string = "\
2
0xce041765675ad4d93378e20bd3a7d0d97ddcf338
0x7f2f54ff94459f3ac4d19d3219ce6ef06868eb8c
0xc2055e4b533b897450a2f7abc14a36882d4a2a10
3
checkmultisig
";

        let pk_1 = PublicKey::from_bytes([1; 33]);
        let pk_2 = PublicKey::from_bytes([2; 33]);
        let pk_3 = PublicKey::from_bytes([3; 33]);

        let redeem_script = &[
            Item::Value(MyValue::Integer(2)),
            Item::Value(MyValue::Bytes(pk_1.pkh().bytes().to_vec())),
            Item::Value(MyValue::Bytes(pk_2.pkh().bytes().to_vec())),
            Item::Value(MyValue::Bytes(pk_3.pkh().bytes().to_vec())),
            Item::Value(MyValue::Integer(3)),
            Item::Operator(MyOperator::CheckMultiSig),
        ];

        assert_eq!(&parse_script(script_string).unwrap(), redeem_script);
    }
}
