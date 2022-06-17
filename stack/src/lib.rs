use scriptful::{core::machine::Machine, core::Script, core::ScriptRef};

pub use crate::error::ScriptError;
pub use crate::operators::{MyOperator, MyValue};
pub use scriptful::core::item::Item;
use witnet_data_structures::chain::Hash;

mod error;
mod operators;
pub mod parser;
#[cfg(test)]
mod tests;

pub fn decode(a: &[u8]) -> Result<Script<MyOperator, MyValue>, ScriptError> {
    let x: Vec<Item<MyOperator, MyValue>> =
        serde_json::from_slice(a).map_err(ScriptError::Decode)?;

    Ok(x)
}

pub fn encode(a: ScriptRef<MyOperator, MyValue>) -> Result<Vec<u8>, ScriptError> {
    // TODO: decide real encoding format, do not use JSON
    // TODO: add version byte to encoded value? To allow easier upgradability
    serde_json::to_vec(a).map_err(ScriptError::Encode)
}

/// Additional state needed for script execution, such as the timestamp of the block that included
/// the script transaction.
#[derive(Default)]
pub struct ScriptContext {
    /// Timestamp of the block that includes the redeem script.
    pub block_timestamp: i64,
    /// Hash of the transaction that includes the redeem script
    pub tx_hash: Hash,
    // TODO: disable signature validation, for testing. Defaults to false.
    pub disable_signature_verify: bool,
}

impl ScriptContext {
    pub fn default_no_signature_verify() -> Self {
        Self {
            disable_signature_verify: true,
            ..Self::default()
        }
    }
}

/// Execute script with context.
///
/// A return value of `Ok` indicates that the script stopped execution because it run out of operators.
/// `Ok(true)` is returned if the stack ends up containing exactly one item, and that item is a boolean true.
/// `Ok(false)` is returned if the stack is empty, if the stack has any value other than true,
/// or if the stack has more than one item.
///
/// A return value of `Err` indicates some problem during script execution that marks the script as
/// failed.
fn execute_script(
    script: ScriptRef<MyOperator, MyValue>,
    context: &ScriptContext,
) -> Result<bool, ScriptError> {
    // Instantiate the machine with a reference to your operator system.
    let mut machine = Machine::new(|a, b, c| operators::my_operator_system(a, b, c, context));
    let res = machine.run_script(script)?;

    // Script execution is considered successful if the stack ends up containing exactly one item,
    // a boolean "true".
    Ok(res == Some(&MyValue::Boolean(true)) && machine.stack_length() == 1)
}

fn execute_locking_script(
    redeem_bytes: &[u8],
    locking_bytes: &[u8; 20],
    context: &ScriptContext,
) -> Result<bool, ScriptError> {
    // Check locking script
    let locking_script = &[
        // Push redeem script as first argument
        Item::Value(MyValue::Bytes(redeem_bytes.to_vec())),
        // Compare hash of redeem script with value of "locking_bytes"
        Item::Operator(MyOperator::Hash160),
        Item::Value(MyValue::Bytes(locking_bytes.to_vec())),
        Item::Operator(MyOperator::Equal),
    ];

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
    execute_script(&witness_script, context)
}

pub fn execute_complete_script(
    witness_bytes: &[u8],
    redeem_bytes: &[u8],
    locking_bytes: &[u8; 20],
    context: &ScriptContext,
) -> Result<bool, ScriptError> {
    // Execute locking script
    let result = execute_locking_script(redeem_bytes, locking_bytes, context)?;
    if !result {
        return Ok(false);
    }

    // Execute witness script concatenated with redeem script
    execute_redeem_script(witness_bytes, redeem_bytes, context)
}
