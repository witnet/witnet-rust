use scriptful::{
    core::{Script, ScriptRef},
    prelude::Stack,
};
use serde::{Deserialize, Serialize};
use std::fmt::Formatter;
use std::marker::PhantomData;

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
    Sha256,
    CheckMultiSig,
    CheckTimeLock,
    /// Stop script execution if top-most element of stack is not "true"
    Verify,
    // Control flow
    If,
    Else,
    EndIf,
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
    /// Signature.
    Signature(Vec<u8>),
}

fn equal_operator(stack: &mut Stack<MyValue>) {
    let a = stack.pop();
    let b = stack.pop();
    stack.push(MyValue::Boolean(a == b));
}

fn hash_160_operator(stack: &mut Stack<MyValue>) {
    let a = stack.pop();
    if let MyValue::Bytes(bytes) = a {
        let mut pkh = [0; 20];
        let Sha256(h) = calculate_sha256(&bytes);
        pkh.copy_from_slice(&h[..20]);
        stack.push(MyValue::Bytes(pkh.as_ref().to_vec()));
    } else {
        // TODO: hash other types?
    }
}

fn sha_256_operator(stack: &mut Stack<MyValue>) {
    let a = stack.pop();
    if let MyValue::Bytes(bytes) = a {
        let Sha256(h) = calculate_sha256(&bytes);
        stack.push(MyValue::Bytes(h.to_vec()));
    } else {
        // TODO: hash other types?
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
            MyValue::Signature(bytes) => {
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

fn check_timelock_operator(stack: &mut Stack<MyValue>, block_timestamp: i64) {
    let timelock = stack.pop();
    match timelock {
        MyValue::Integer(timelock) => {
            let timelock_ok = i128::from(block_timestamp) >= timelock;
            stack.push(MyValue::Boolean(timelock_ok));
        }
        _ => {
            // TODO change panic by error
            unreachable!("CheckTimelock should pick an integer as a first value");
        }
    }
}

// An operator system decides what to do with the stack when each operator is applied on it.
fn my_operator_system(
    stack: &mut Stack<MyValue>,
    operator: &MyOperator,
    if_stack: &mut ConditionStack,
    context: &ScriptContext,
) -> MyControlFlow {
    if !if_stack.all_true() {
        match operator {
            MyOperator::If => {
                if_stack.push_back(false);
            }
            MyOperator::Else => {
                if if_stack.toggle_top().is_none() {
                    stack.push(MyValue::Boolean(false));
                    return MyControlFlow::Break;
                }
            }
            MyOperator::EndIf => {
                if if_stack.pop_back().is_none() {
                    stack.push(MyValue::Boolean(false));
                    return MyControlFlow::Break;
                }
            }
            _ => {}
        }

        return MyControlFlow::Continue;
    }

    match operator {
        MyOperator::Equal => equal_operator(stack),
        MyOperator::Hash160 => hash_160_operator(stack),
        MyOperator::Sha256 => sha_256_operator(stack),
        MyOperator::CheckMultiSig => check_multisig_operator(stack),
        MyOperator::CheckTimeLock => check_timelock_operator(stack, context.block_timestamp),
        MyOperator::Verify => {
            let top = stack.pop();
            if top != MyValue::Boolean(true) {
                // Push the element back because there is a check in execute_script that needs a
                // false value to mark the script execution as failed, otherwise it may be marked as
                // success
                stack.push(top);
                return MyControlFlow::Break;
            }
        }
        MyOperator::If => {
            let top = stack.pop();
            if let MyValue::Boolean(b) = top {
                if_stack.push_back(b);
            } else {
                stack.push(MyValue::Boolean(false));
                return MyControlFlow::Break;
            }
        }
        MyOperator::Else => {
            if if_stack.toggle_top().is_none() {
                stack.push(MyValue::Boolean(false));
                return MyControlFlow::Break;
            }
        }
        MyOperator::EndIf => {
            if if_stack.pop_back().is_none() {
                stack.push(MyValue::Boolean(false));
                return MyControlFlow::Break;
            }
        }
    }

    MyControlFlow::Continue
}

// ConditionStack implementation from bitcoin-core
// https://github.com/bitcoin/bitcoin/blob/505ba3966562b10d6dd4162f3216a120c73a4edb/src/script/interpreter.cpp#L272
// https://bitslog.com/2017/04/17/new-quadratic-delays-in-bitcoin-scripts/
/** A data type to abstract out the condition stack during script execution.
*
* Conceptually it acts like a vector of booleans, one for each level of nested
* IF/THEN/ELSE, indicating whether we're in the active or inactive branch of
* each.
*
* The elements on the stack cannot be observed individually; we only need to
* expose whether the stack is empty and whether or not any false values are
* present at all. To implement OP_ELSE, a toggle_top modifier is added, which
* flips the last value without returning it.
*
* This uses an optimized implementation that does not materialize the
* actual stack. Instead, it just stores the size of the would-be stack,
* and the position of the first false value in it.
 */
pub struct ConditionStack {
    stack_size: u32,
    first_false_pos: u32,
}

impl Default for ConditionStack {
    fn default() -> Self {
        Self {
            stack_size: 0,
            first_false_pos: Self::NO_FALSE,
        }
    }
}

impl ConditionStack {
    const NO_FALSE: u32 = u32::MAX;

    pub fn is_empty(&self) -> bool {
        self.stack_size == 0
    }

    pub fn all_true(&self) -> bool {
        self.first_false_pos == Self::NO_FALSE
    }

    pub fn push_back(&mut self, b: bool) {
        if (self.first_false_pos == Self::NO_FALSE) && !b {
            // The stack consists of all true values, and a false is added.
            // The first false value will appear at the current size.
            self.first_false_pos = self.stack_size;
        }

        self.stack_size += 1;
    }

    pub fn pop_back(&mut self) -> Option<()> {
        if self.stack_size == 0 {
            return None;
        }

        self.stack_size -= 1;
        if self.first_false_pos == self.stack_size {
            // When popping off the first false value, everything becomes true.
            self.first_false_pos = Self::NO_FALSE;
        }

        Some(())
    }

    pub fn toggle_top(&mut self) -> Option<()> {
        if self.stack_size == 0 {
            return None;
        }

        if self.first_false_pos == Self::NO_FALSE {
            // The current stack is all true values; the first false will be the top.
            self.first_false_pos = self.stack_size - 1;
        } else if self.first_false_pos == self.stack_size - 1 {
            // The top is the first false value; toggling it will make everything true.
            self.first_false_pos = Self::NO_FALSE;
        } else {
            // There is a false value, but not on top. No action is needed as toggling
            // anything but the first false value is unobservable.
        }

        Some(())
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

pub fn decode(a: &[u8]) -> Result<Script<MyOperator, MyValue>, ScriptError> {
    let x: Vec<Item2<MyOperator, MyValue>> =
        serde_json::from_slice(a).map_err(ScriptError::Decode)?;

    Ok(x.into_iter().map(Into::into).collect())
}

pub fn encode(a: Script<MyOperator, MyValue>) -> Result<Vec<u8>, ScriptError> {
    let x: Vec<Item2<MyOperator, MyValue>> = a.into_iter().map(Into::into).collect();

    serde_json::to_vec(&x).map_err(ScriptError::Encode)
}

#[derive(Default)]
pub struct ScriptContext {
    pub block_timestamp: i64,
}

#[derive(Debug)]
pub enum ScriptError {
    Decode(serde_json::Error),
    Encode(serde_json::Error),
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptError::Decode(e) => write!(f, "Decode script failed: {}", e),
            ScriptError::Encode(e) => write!(f, "Encode script failed: {}", e),
        }
    }
}

impl std::error::Error for ScriptError {}

fn execute_script(script: Script<MyOperator, MyValue>, context: &ScriptContext) -> bool {
    // Instantiate the machine with a reference to your operator system.
    let mut machine = Machine2::new(|a, b, c| my_operator_system(a, b, c, context));
    let result = machine.run_script(&script);

    result == None || result == Some(&MyValue::Boolean(true))
}

fn execute_locking_script(
    redeem_bytes: &[u8],
    locking_bytes: &[u8; 20],
    context: &ScriptContext,
) -> bool {
    // Check locking script
    let mut locking_script = vec![
        Item::Operator(MyOperator::Hash160),
        Item::Value(MyValue::Bytes(locking_bytes.to_vec())),
        Item::Operator(MyOperator::Equal),
    ];

    // Push redeem script as argument
    locking_script.insert(0, Item::Value(MyValue::Bytes(redeem_bytes.to_vec())));

    // Execute the script
    execute_script(locking_script, context)
}

fn execute_redeem_script(
    witness_bytes: &[u8],
    redeem_bytes: &[u8],
    context: &ScriptContext,
) -> Result<bool, ScriptError> {
    // Execute witness script concatenated with redeem script
    let mut witness_script = decode(witness_bytes)?;
    let redeem_script = decode(redeem_bytes)?;
    witness_script.extend(redeem_script);

    // Execute the script
    Ok(execute_script(witness_script, context))
}

pub fn execute_complete_script(
    witness_bytes: &[u8],
    redeem_bytes: &[u8],
    locking_bytes: &[u8; 20],
    context: &ScriptContext,
) -> Result<bool, ScriptError> {
    // Execute locking script
    let result = execute_locking_script(redeem_bytes, locking_bytes, context);
    if !result {
        return Ok(false);
    }

    // Execute witness script concatenated with redeem script
    execute_redeem_script(witness_bytes, redeem_bytes, context)
}

// TODO: use control flow enum from scriptful library when ready
pub enum MyControlFlow {
    Continue,
    Break,
}

pub struct Machine2<Op, Val, F>
where
    Val: core::fmt::Debug + core::cmp::PartialEq,
    F: FnMut(&mut Stack<Val>, &Op, &mut ConditionStack) -> MyControlFlow,
{
    op_sys: F,
    stack: Stack<Val>,
    if_stack: ConditionStack,
    phantom_op: PhantomData<fn(&Op)>,
}

impl<Op, Val, F> Machine2<Op, Val, F>
where
    Op: core::fmt::Debug + core::cmp::Eq,
    Val: core::fmt::Debug + core::cmp::PartialEq + core::clone::Clone,
    F: FnMut(&mut Stack<Val>, &Op, &mut ConditionStack) -> MyControlFlow,
{
    pub fn new(op_sys: F) -> Self {
        Self {
            op_sys,
            stack: Stack::<Val>::default(),
            if_stack: ConditionStack::default(),
            phantom_op: PhantomData,
        }
    }

    pub fn operate(&mut self, item: &Item<Op, Val>) -> MyControlFlow {
        match item {
            Item::Operator(operator) => {
                (self.op_sys)(&mut self.stack, operator, &mut self.if_stack)
            }
            Item::Value(value) => {
                if self.if_stack.all_true() {
                    self.stack.push((*value).clone());
                }

                MyControlFlow::Continue
            }
        }
    }

    pub fn run_script(&mut self, script: ScriptRef<Op, Val>) -> Option<&Val> {
        for item in script {
            match self.operate(item) {
                MyControlFlow::Continue => {
                    continue;
                }
                MyControlFlow::Break => {
                    break;
                }
            }
        }

        self.stack.topmost()
    }
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
        assert!(execute_script(s, &ScriptContext::default()));

        let s = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("potato".to_string())),
            Item::Operator(MyOperator::Equal),
        ];
        assert!(!execute_script(s, &ScriptContext::default()));
    }

    #[test]
    fn test_execute_locking_script() {
        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        let locking_script = EQUAL_OPERATOR_HASH;
        assert!(execute_locking_script(
            &encode(redeem_script).unwrap(),
            &locking_script,
            &ScriptContext::default(),
        ));

        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        let locking_script = [1; 20];
        assert!(!execute_locking_script(
            &encode(redeem_script).unwrap(),
            &locking_script,
            &ScriptContext::default(),
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
            &encode(witness).unwrap(),
            &encode(redeem_script).unwrap(),
            &ScriptContext::default(),
        )
        .unwrap());

        let witness = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("potato".to_string())),
        ];
        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        assert!(!execute_redeem_script(
            &encode(witness).unwrap(),
            &encode(redeem_script).unwrap(),
            &ScriptContext::default(),
        )
        .unwrap());
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
            &encode(witness).unwrap(),
            &encode(redeem_script).unwrap(),
            &locking_script,
            &ScriptContext::default(),
        )
        .unwrap());

        let witness = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("potato".to_string())),
        ];
        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        let locking_script = EQUAL_OPERATOR_HASH;
        assert!(!execute_complete_script(
            &encode(witness).unwrap(),
            &encode(redeem_script).unwrap(),
            &locking_script,
            &ScriptContext::default(),
        )
        .unwrap());

        let witness = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("patata".to_string())),
        ];
        let redeem_script = vec![Item::Operator(MyOperator::Equal)];
        let locking_script: [u8; 20] = [1; 20];
        assert!(!execute_complete_script(
            &encode(witness).unwrap(),
            &encode(redeem_script).unwrap(),
            &locking_script,
            &ScriptContext::default(),
        )
        .unwrap());
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
            Item::Value(MyValue::Signature(ks_1.to_pb_bytes().unwrap())),
            Item::Value(MyValue::Signature(ks_2.to_pb_bytes().unwrap())),
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
            &encode(witness).unwrap(),
            &encode(redeem_script).unwrap(),
            &ScriptContext::default(),
        )
        .unwrap());

        let other_valid_witness = vec![
            Item::Value(MyValue::Signature(ks_1.to_pb_bytes().unwrap())),
            Item::Value(MyValue::Signature(ks_3.to_pb_bytes().unwrap())),
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
            &encode(other_valid_witness).unwrap(),
            &encode(redeem_script).unwrap(),
            &ScriptContext::default(),
        )
        .unwrap());

        let pk_4 = PublicKey::from_bytes([4; 33]);
        let ks_4 = ks_from_pk(pk_4);
        let invalid_witness = vec![
            Item::Value(MyValue::Signature(ks_1.to_pb_bytes().unwrap())),
            Item::Value(MyValue::Signature(ks_4.to_pb_bytes().unwrap())),
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
            &encode(invalid_witness).unwrap(),
            &encode(redeem_script).unwrap(),
            &ScriptContext::default(),
        )
        .unwrap());
    }

    #[test]
    fn test_execute_script_op_verify() {
        let s = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("patata".to_string())),
            Item::Operator(MyOperator::Equal),
            Item::Operator(MyOperator::Verify),
        ];
        assert!(execute_script(s, &ScriptContext::default()));

        let s = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::String("potato".to_string())),
            Item::Operator(MyOperator::Equal),
            Item::Operator(MyOperator::Verify),
        ];
        assert!(!execute_script(s, &ScriptContext::default()));
    }

    #[test]
    fn test_execute_script_op_if() {
        let s = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::Boolean(true)),
            Item::Operator(MyOperator::If),
            Item::Value(MyValue::String("patata".to_string())),
            Item::Operator(MyOperator::Else),
            Item::Value(MyValue::String("potato".to_string())),
            Item::Operator(MyOperator::EndIf),
            Item::Operator(MyOperator::Equal),
            Item::Operator(MyOperator::Verify),
        ];
        assert!(execute_script(s, &ScriptContext::default()));

        let s = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::Boolean(false)),
            Item::Operator(MyOperator::If),
            Item::Value(MyValue::String("patata".to_string())),
            Item::Operator(MyOperator::Else),
            Item::Value(MyValue::String("potato".to_string())),
            Item::Operator(MyOperator::EndIf),
            Item::Operator(MyOperator::Equal),
            Item::Operator(MyOperator::Verify),
        ];
        assert!(!execute_script(s, &ScriptContext::default()));
    }

    #[test]
    fn test_execute_script_op_if_nested() {
        let s = vec![
            Item::Value(MyValue::String("patata".to_string())),
            Item::Value(MyValue::Boolean(true)),
            Item::Operator(MyOperator::If),
            Item::Value(MyValue::String("patata".to_string())),
            Item::Operator(MyOperator::Else),
            Item::Value(MyValue::Boolean(false)),
            Item::Operator(MyOperator::If),
            Item::Value(MyValue::String("patata".to_string())),
            Item::Operator(MyOperator::Else),
            Item::Value(MyValue::String("potato".to_string())),
            Item::Operator(MyOperator::EndIf),
            Item::Operator(MyOperator::EndIf),
            Item::Operator(MyOperator::Equal),
            Item::Operator(MyOperator::Verify),
        ];
        assert!(execute_script(s, &ScriptContext::default()));

        let s = vec![
            Item::Value(MyValue::String("potato".to_string())),
            Item::Value(MyValue::Boolean(false)),
            Item::Operator(MyOperator::If),
            Item::Value(MyValue::String("patata".to_string())),
            Item::Operator(MyOperator::Else),
            Item::Value(MyValue::Boolean(false)),
            Item::Operator(MyOperator::If),
            Item::Value(MyValue::String("patata".to_string())),
            Item::Operator(MyOperator::Else),
            Item::Value(MyValue::String("potato".to_string())),
            Item::Operator(MyOperator::EndIf),
            Item::Operator(MyOperator::EndIf),
            Item::Operator(MyOperator::Equal),
            Item::Operator(MyOperator::Verify),
        ];
        assert!(execute_script(s, &ScriptContext::default()));
    }

    #[test]
    fn machine_with_context() {
        let mut v = vec![0u32];
        let mut m = Machine2::new(|_stack: &mut Stack<()>, operator, _if_stack| {
            v.push(*operator);

            MyControlFlow::Continue
        });
        m.run_script(&[Item::Operator(1), Item::Operator(3), Item::Operator(2)]);

        assert_eq!(v, vec![0, 1, 3, 2]);
    }

    #[test]
    fn test_execute_script_op_check_timelock() {
        let s = vec![
            Item::Value(MyValue::Integer(10_000)),
            Item::Operator(MyOperator::CheckTimeLock),
            Item::Operator(MyOperator::Verify),
        ];
        assert!(!execute_script(s, &ScriptContext { block_timestamp: 0 }));

        let s = vec![
            Item::Value(MyValue::Integer(10_000)),
            Item::Operator(MyOperator::CheckTimeLock),
            Item::Operator(MyOperator::Verify),
        ];
        assert!(execute_script(
            s,
            &ScriptContext {
                block_timestamp: 20_000,
            }
        ));
    }
}
