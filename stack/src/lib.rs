use scriptful::{
    core::Script,
    prelude::{Machine, Stack},
};
use serde::{Deserialize, Serialize};

use witnet_crypto::hash::{calculate_sha256, Sha256};
use witnet_data_structures::{
    chain::{KeyedSignature, PublicKeyHash},
    proto::ProtobufConvert,
};

pub use scriptful::prelude::Item;

// You can define your own operators.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
// TODO: Include more operators
pub enum MyOperator {
    Equal,
    Hash160,
    CheckMultiSig,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum MyValue {
    /// A binary value: either `true` or `false`.
    Boolean(bool),
    /// A signed floating point value.
    Float(f64),
    /// A signed integer value.
    Integer(i128),
    /// A string of characters.
    String(String),
    /// Bytes.
    Bytes(Vec<u8>),
}

fn equal_operator(stack: &mut Stack<MyValue>) {
    let a = stack.pop();
    let b = stack.pop();
    stack.push(MyValue::Boolean(a == b));
}

fn hash_160_operator(stack: &mut Stack<MyValue>) {
    let a = stack.pop();
    match a {
        MyValue::Boolean(_) => {}
        MyValue::Float(_) => {}
        MyValue::Integer(_) => {}
        MyValue::String(_) => {}
        MyValue::Bytes(bytes) => {
            let mut pkh = [0; 20];
            let Sha256(h) = calculate_sha256(&bytes);
            pkh.copy_from_slice(&h[..20]);
            stack.push(MyValue::Bytes(pkh.as_ref().to_vec()));
        }
    }
}

fn check_multisig_operator(stack: &mut Stack<MyValue>) {
    let m = stack.pop();
    match m {
        MyValue::Integer(m) => {
            let mut pkhs = vec![];
            for _ in 0..m {
                pkhs.push(stack.pop());
            }

            let n = stack.pop();
            match n {
                MyValue::Integer(n) => {
                    let mut keyed_signatures = vec![];
                    for _ in 0..n {
                        keyed_signatures.push(stack.pop());
                    }

                    let res = check_multi_sig(pkhs, keyed_signatures);
                    stack.push(MyValue::Boolean(res));
                }
                _ => {
                    // TODO change panic by error
                    unreachable!("CheckMultisig should pick an integer as a ~second~ value");
                }
            }
        }
        _ => {
            // TODO change panic by error
            unreachable!("CheckMultisig should pick an integer as a first value");
        }
    }
}

fn check_multi_sig(bytes_pkhs: Vec<MyValue>, bytes_keyed_signatures: Vec<MyValue>) -> bool {
    let mut signed_pkhs = vec![];
    for signature in bytes_keyed_signatures {
        match signature {
            MyValue::Bytes(bytes) => {
                // TODO Handle unwrap
                let ks: KeyedSignature = KeyedSignature::from_pb_bytes(&bytes).unwrap();
                let signed_pkh = ks.public_key.pkh();
                signed_pkhs.push(signed_pkh);
            }
            _ => {
                // TODO change panic by error
                unreachable!("check_multi_sig should pick only bytes");
            }
        }
    }

    let mut pkhs = vec![];
    for bytes_pkh in bytes_pkhs {
        match bytes_pkh {
            MyValue::Bytes(bytes) => {
                // TODO Handle unwrap
                let pkh: PublicKeyHash = PublicKeyHash::from_bytes(&bytes).unwrap();
                pkhs.push(pkh);
            }
            _ => {
                // TODO change panic by error
                unreachable!("check_multi_sig should pick only bytes");
            }
        }
    }

    for sign_pkh in signed_pkhs {
        let pos = pkhs.iter().position(|&x| x == sign_pkh);

        match pos {
            Some(i) => {
                pkhs.remove(i);
            }
            None => {
                return false;
            }
        }
    }

    true
}

// An operator system decides what to do with the stack when each operator is applied on it.
fn my_operator_system(stack: &mut Stack<MyValue>, operator: &MyOperator) {
    match operator {
        MyOperator::Equal => equal_operator(stack),
        MyOperator::Hash160 => hash_160_operator(stack),
        MyOperator::CheckMultiSig => check_multisig_operator(stack),
    }
}

#[derive(Clone, Deserialize, Serialize)]
enum Item2<Op, Val>
where
    Op: core::fmt::Debug,
    Val: core::fmt::Debug,
{
    Operator(Op),
    Value(Val),
}

impl<Op, Val> From<Item2<Op, Val>> for Item<Op, Val>
where
    Op: core::fmt::Debug,
    Val: core::fmt::Debug,
{
    fn from(x: Item2<Op, Val>) -> Self {
        match x {
            Item2::Operator(op) => Item::Operator(op),
            Item2::Value(val) => Item::Value(val),
        }
    }
}

impl<Op, Val> From<Item<Op, Val>> for Item2<Op, Val>
where
    Op: core::fmt::Debug,
    Val: core::fmt::Debug,
{
    fn from(x: Item<Op, Val>) -> Self {
        match x {
            Item::Operator(op) => Item2::Operator(op),
            Item::Value(val) => Item2::Value(val),
        }
    }
}

pub fn decode(a: &[u8]) -> Script<MyOperator, MyValue> {
    let x: Vec<Item2<MyOperator, MyValue>> = serde_json::from_slice(a).unwrap();

    x.into_iter().map(Into::into).collect()
}

pub fn encode(a: Script<MyOperator, MyValue>) -> Vec<u8> {
    let x: Vec<Item2<MyOperator, MyValue>> = a.into_iter().map(Into::into).collect();
    serde_json::to_vec(&x).unwrap()
}

fn execute_script(script: Script<MyOperator, MyValue>) -> bool {
    // Instantiate the machine with a reference to your operator system.
    let mut machine = Machine::new(&my_operator_system);
    let result = machine.run_script(&script);

    result == Some(&MyValue::Boolean(true))
}

fn execute_locking_script(redeem_bytes: &[u8], locking_bytes: &[u8; 20]) -> bool {
    // Check locking script
    let mut locking_script = vec![
        Item::Operator(MyOperator::Hash160),
        Item::Value(MyValue::Bytes(locking_bytes.to_vec())),
        Item::Operator(MyOperator::Equal),
    ];

    // Push redeem script as argument
    locking_script.insert(0, Item::Value(MyValue::Bytes(redeem_bytes.to_vec())));

    // Execute the script
    execute_script(locking_script)
}

fn execute_redeem_script(witness_bytes: &[u8], redeem_bytes: &[u8]) -> bool {
    // Execute witness script concatenated with redeem script
    let mut witness_script = decode(witness_bytes);
    let redeem_script = decode(redeem_bytes);
    witness_script.extend(redeem_script);

    // Execute the script
    execute_script(witness_script)
}

pub fn execute_complete_script(
    witness_bytes: &[u8],
    redeem_bytes: &[u8],
    locking_bytes: &[u8; 20],
) -> bool {
    // Execute locking script
    let result = execute_locking_script(redeem_bytes, locking_bytes);
    if !result {
        return false;
    }

    // Execute witness script concatenated with redeem script
    execute_redeem_script(witness_bytes, redeem_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute_script;
    use witnet_data_structures::chain::PublicKey;
    const EQUAL_OPERATOR_HASH: [u8; 20] = [
        52, 128, 191, 80, 253, 28, 169, 253, 237, 29, 0, 51, 201, 0, 31, 203, 157, 99, 218, 210,
    ];

    #[test]
    fn test_execute_script() {
        let s = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("patata".to_string())),
            Item::Operator(MyOperator::Equal),
        ];
        assert!(execute_script(s));

        let s = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("potato".to_string())),
            Item::Operator(MyOperator::Equal),
        ];
        assert!(!execute_script(s));
    }

    #[test]
    fn test_execute_locking_script() {
        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        let locking_script = EQUAL_OPERATOR_HASH;
        assert!(execute_locking_script(
            &encode(redeem_script),
            &locking_script
        ));

        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        let locking_script = [1; 20];
        assert!(!execute_locking_script(
            &encode(redeem_script),
            &locking_script
        ));
    }

    #[test]
    fn test_execute_redeem_script() {
        let witness = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("patata".to_string())),
        ];
        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        assert!(execute_redeem_script(
            &encode(witness),
            &encode(redeem_script)
        ));

        let witness = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("potato".to_string())),
        ];
        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        assert!(!execute_redeem_script(
            &encode(witness),
            &encode(redeem_script)
        ));
    }

    #[test]
    fn test_complete_script() {
        let witness = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("patata".to_string())),
        ];
        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        let locking_script = EQUAL_OPERATOR_HASH;
        assert!(execute_complete_script(
            &encode(witness),
            &encode(redeem_script),
            &locking_script,
        ));

        let witness = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("potato".to_string())),
        ];
        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        let locking_script = EQUAL_OPERATOR_HASH;
        assert!(!execute_complete_script(
            &encode(witness),
            &encode(redeem_script),
            &locking_script,
        ));

        let witness = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("patata".to_string())),
        ];
        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        let locking_script: [u8; 20] = [1; 20];
        assert!(!execute_complete_script(
            &encode(witness),
            &encode(redeem_script),
            &locking_script,
        ));
    }

    fn ks_from_pk(pk: PublicKey) -> KeyedSignature {
        KeyedSignature {
            signature: Default::default(),
            public_key: pk,
        }
    }
    #[test]
    fn test_check_multisig() {
        let pk_1 = PublicKey::from_bytes([1; 33]);
        let pk_2 = PublicKey::from_bytes([2; 33]);
        let pk_3 = PublicKey::from_bytes([3; 33]);

        let ks_1 = ks_from_pk(pk_1.clone());
        let ks_2 = ks_from_pk(pk_2.clone());
        let ks_3 = ks_from_pk(pk_3.clone());

        let witness = vec![
            Item::Value(MyValue::Bytes(ks_1.to_pb_bytes().unwrap())),
            Item::Value(MyValue::Bytes(ks_2.to_pb_bytes().unwrap())),
        ];
        let redeem_script = vec![
            Item::Value(MyValue::Integer(2)),
            Item::Value(MyValue::Bytes(pk_1.pkh().bytes().to_vec())),
            Item::Value(MyValue::Bytes(pk_2.pkh().bytes().to_vec())),
            Item::Value(MyValue::Bytes(pk_3.pkh().bytes().to_vec())),
            Item::Value(MyValue::Integer(3)),
            Item::Operator(MyOperator::CheckMultiSig),
        ];
        assert!(execute_redeem_script(
            &encode(witness),
            &encode(redeem_script)
        ));

        let other_valid_witness = vec![
            Item::Value(MyValue::Bytes(ks_1.to_pb_bytes().unwrap())),
            Item::Value(MyValue::Bytes(ks_3.to_pb_bytes().unwrap())),
        ];
        let redeem_script = vec![
            Item::Value(MyValue::Integer(2)),
            Item::Value(MyValue::Bytes(pk_1.pkh().bytes().to_vec())),
            Item::Value(MyValue::Bytes(pk_2.pkh().bytes().to_vec())),
            Item::Value(MyValue::Bytes(pk_3.pkh().bytes().to_vec())),
            Item::Value(MyValue::Integer(3)),
            Item::Operator(MyOperator::CheckMultiSig),
        ];
        assert!(execute_redeem_script(
            &encode(other_valid_witness),
            &encode(redeem_script)
        ));

        let pk_4 = PublicKey::from_bytes([4; 33]);
        let ks_4 = ks_from_pk(pk_4);
        let invalid_witness = vec![
            Item::Value(MyValue::Bytes(ks_1.to_pb_bytes().unwrap())),
            Item::Value(MyValue::Bytes(ks_4.to_pb_bytes().unwrap())),
        ];
        let redeem_script = vec![
            Item::Value(MyValue::Integer(2)),
            Item::Value(MyValue::Bytes(pk_1.pkh().bytes().to_vec())),
            Item::Value(MyValue::Bytes(pk_2.pkh().bytes().to_vec())),
            Item::Value(MyValue::Bytes(pk_3.pkh().bytes().to_vec())),
            Item::Value(MyValue::Integer(3)),
            Item::Operator(MyOperator::CheckMultiSig),
        ];
        assert!(!execute_redeem_script(
            &encode(invalid_witness),
            &encode(redeem_script)
        ));
    }
}
