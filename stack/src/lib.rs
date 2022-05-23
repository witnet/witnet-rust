use scriptful::{
    core::Script,
    prelude::{Machine, Stack},
};
use serde::{Deserialize, Serialize};

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

fn hash_160_operator(_stack: &mut Stack<MyValue>) {
    // TODO: implement it
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

pub fn execute_script(script: Script<MyOperator, MyValue>) -> bool {
    // Instantiate the machine with a reference to your operator system.
    let mut machine = Machine::new(&my_operator_system);
    let result = machine.run_script(&script);

    result == Some(&MyValue::Boolean(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute_script;

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
}
