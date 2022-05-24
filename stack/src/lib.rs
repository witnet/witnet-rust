use scriptful::{
    core::Script,
    prelude::{Machine, Stack},
};
use serde::{Deserialize, Serialize};

use witnet_crypto::hash::{calculate_sha256, Sha256};

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

fn check_multisig_operator(_stack: &mut Stack<MyValue>) {
    // TODO: implement it
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
}
