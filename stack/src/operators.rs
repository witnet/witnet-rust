use crate::{ScriptContext, ScriptError};
use scriptful::prelude::{ConditionStack, Stack};
use serde::{Deserialize, Serialize};
use witnet_crypto::hash::{calculate_sha256, Sha256};
use witnet_data_structures::chain::{Hash, PublicKey, Secp256k1Signature, Signature};
use witnet_data_structures::chain::{KeyedSignature, PublicKeyHash};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
// TODO: Include more operators
pub enum MyOperator {
    /// Pop two elements from the stack, push boolean indicating whether they are equal.
    Equal,
    /// Pop bytes from the stack and apply SHA-256 truncated to 160 bits. This is the hash used in Witnet to calculate a PublicKeyHash from a PublicKey.
    Hash160,
    /// Pop bytes from the stack and apply SHA-256.
    Sha256,
    /// Pop PublicKeyHash and Signature from the stack, push boolean indicating whether the signature is valid.
    CheckSig,
    /// Pop integer "n", n PublicKeyHashes, integer "m" and m Signatures. Push boolean indicating whether the signatures are valid.
    CheckMultiSig,
    /// Pop integer "timelock" from the stack, push boolean indicating whether the block timestamp is greater than the timelock.
    CheckTimeLock,
    /// Pop element from the stack and stop script execution if that element is not "true".
    Verify,
    /// Pop boolean from the stack and conditionally execute the next If block.
    If,
    /// Flip execution condition inside an If block.
    Else,
    /// Mark end of If block.
    EndIf,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum MyValue {
    /// A binary value: either `true` or `false`.
    Boolean(bool),
    /// A signed integer value.
    Integer(i128),
    /// Bytes.
    Bytes(Vec<u8>),
}

impl MyValue {
    pub fn from_signature(ks: &KeyedSignature) -> Self {
        let public_key_bytes = ks.public_key.to_bytes();
        let signature_bytes = match &ks.signature {
            Signature::Secp256k1(signature) => &signature.der,
        };

        let bytes = [&public_key_bytes[..], signature_bytes].concat();

        MyValue::Bytes(bytes)
    }

    pub fn to_signature(&self) -> Result<KeyedSignature, ScriptError> {
        match self {
            MyValue::Bytes(bytes) => {
                // Public keys are always 33 bytes, so first 33 bytes of KeyedSignature will always
                // be the public key, and the rest will be the signature
                if bytes.len() < 33 {
                    return Err(ScriptError::InvalidSignature);
                }
                let (public_key_bytes, signature_bytes) = bytes.split_at(33);

                let ks = KeyedSignature {
                    public_key: PublicKey::try_from_slice(public_key_bytes)
                        .expect("public_key_bytes must have length 33"),
                    signature: Signature::Secp256k1(Secp256k1Signature {
                        der: signature_bytes.to_vec(),
                    }),
                };

                Ok(ks)
            }
            _ => Err(ScriptError::UnexpectedArgument),
        }
    }
}

fn equal_operator(stack: &mut Stack<MyValue>) -> Result<(), ScriptError> {
    let a = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
    let b = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
    stack.push(MyValue::Boolean(a == b));

    Ok(())
}

fn hash_160_operator(stack: &mut Stack<MyValue>) -> Result<(), ScriptError> {
    let a = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
    if let MyValue::Bytes(bytes) = a {
        let mut pkh = [0; 20];
        let Sha256(h) = calculate_sha256(&bytes);
        pkh.copy_from_slice(&h[..20]);
        stack.push(MyValue::Bytes(pkh.as_ref().to_vec()));

        Ok(())
    } else {
        // Only Bytes can be hashed
        Err(ScriptError::UnexpectedArgument)
    }
}

fn sha_256_operator(stack: &mut Stack<MyValue>) -> Result<(), ScriptError> {
    let a = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
    if let MyValue::Bytes(bytes) = a {
        let Sha256(h) = calculate_sha256(&bytes);
        stack.push(MyValue::Bytes(h.to_vec()));

        Ok(())
    } else {
        // Only Bytes can be hashed
        Err(ScriptError::UnexpectedArgument)
    }
}

fn check_sig_operator(
    stack: &mut Stack<MyValue>,
    tx_hash: Hash,
    disable_signature_verify: bool,
) -> Result<(), ScriptError> {
    let pkh = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
    let keyed_signature = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
    // CheckSig operator is validated as a 1-of-1 multisig
    let res = check_multi_sig(
        vec![pkh],
        vec![keyed_signature],
        tx_hash,
        disable_signature_verify,
    )?;
    stack.push(MyValue::Boolean(res));

    Ok(())
}

fn check_multisig_operator(
    stack: &mut Stack<MyValue>,
    tx_hash: Hash,
    disable_signature_verify: bool,
) -> Result<(), ScriptError> {
    let m = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
    match m {
        MyValue::Integer(m) => {
            if m <= 0 || m > 20 {
                return Err(ScriptError::BadNumberPublicKeysInMultiSig);
            }
            let mut pkhs = vec![];
            for _ in 0..m {
                pkhs.push(stack.pop().ok_or(ScriptError::EmptyStackPop)?);
            }

            let n = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
            match n {
                MyValue::Integer(n) => {
                    if n <= 0 || n > 20 {
                        return Err(ScriptError::BadNumberPublicKeysInMultiSig);
                    }
                    let mut keyed_signatures = vec![];
                    for _ in 0..n {
                        keyed_signatures.push(stack.pop().ok_or(ScriptError::EmptyStackPop)?);
                    }

                    let res =
                        check_multi_sig(pkhs, keyed_signatures, tx_hash, disable_signature_verify)?;
                    stack.push(MyValue::Boolean(res));

                    Ok(())
                }
                _ => Err(ScriptError::UnexpectedArgument),
            }
        }
        _ => Err(ScriptError::UnexpectedArgument),
    }
}

fn check_multi_sig(
    bytes_pkhs: Vec<MyValue>,
    bytes_keyed_signatures: Vec<MyValue>,
    tx_hash: Hash,
    disable_signature_verify: bool,
) -> Result<bool, ScriptError> {
    let mut signed_pkhs = vec![];
    let mut keyed_signatures = vec![];
    for value in bytes_keyed_signatures {
        let ks = value.to_signature()?;
        let signed_pkh = ks.public_key.pkh();
        signed_pkhs.push(signed_pkh);
        let signature = ks
            .signature
            .clone()
            .try_into()
            .map_err(|_e| ScriptError::InvalidSignature)?;
        let public_key = ks
            .public_key
            .clone()
            .try_into()
            .map_err(|_e| ScriptError::InvalidPublicKey)?;
        keyed_signatures.push((signature, public_key));
    }

    let mut pkhs = vec![];
    for bytes_pkh in bytes_pkhs {
        match bytes_pkh {
            MyValue::Bytes(bytes) => {
                let pkh: PublicKeyHash = PublicKeyHash::from_bytes(&bytes)
                    .map_err(|_e| ScriptError::InvalidPublicKeyHash)?;
                pkhs.push(pkh);
            }
            _ => {
                return Err(ScriptError::UnexpectedArgument);
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
                // TODO: return Ok(false) if the signature is in valid format but uses a different public key?
                return Err(ScriptError::WrongSignaturePublicKey);
            }
        }
    }

    if disable_signature_verify {
        return Ok(true);
    }

    // Validate signatures and return Ok(false) if any of the signatures is not valid
    for (signature, public_key) in keyed_signatures {
        let sign_data = tx_hash.as_ref();

        if witnet_crypto::signature::verify(&public_key, sign_data, &signature).is_err() {
            return Ok(false);
        }
    }

    Ok(true)
}

fn check_timelock_operator(
    stack: &mut Stack<MyValue>,
    block_timestamp: i64,
) -> Result<(), ScriptError> {
    let timelock = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
    match timelock {
        MyValue::Integer(timelock) => {
            let timelock_ok = i128::from(block_timestamp) >= timelock;
            stack.push(MyValue::Boolean(timelock_ok));
            Ok(())
        }
        _ => Err(ScriptError::UnexpectedArgument),
    }
}

fn verify_operator(stack: &mut Stack<MyValue>) -> Result<(), ScriptError> {
    let top = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
    if top != MyValue::Boolean(true) {
        return Err(ScriptError::VerifyOpFailed);
    }

    Ok(())
}

fn if_operator(
    stack: &mut Stack<MyValue>,
    if_stack: &mut ConditionStack,
) -> Result<(), ScriptError> {
    if if_stack.all_true() {
        let top = stack.pop().ok_or(ScriptError::EmptyStackPop)?;
        if let MyValue::Boolean(b) = top {
            if_stack.push_back(b);
        } else {
            return Err(ScriptError::IfNotBoolean);
        }
    } else {
        // Avoid touching the stack if execution is disabled
        if_stack.push_back(false);
    }

    Ok(())
}

fn else_operator(if_stack: &mut ConditionStack) -> Result<(), ScriptError> {
    if if_stack.toggle_top().is_none() {
        return Err(ScriptError::UnbalancedElseOp);
    }

    Ok(())
}

fn end_if_operator(if_stack: &mut ConditionStack) -> Result<(), ScriptError> {
    if if_stack.pop_back().is_none() {
        return Err(ScriptError::UnbalancedEndIfOp);
    }

    Ok(())
}

// An operator system decides what to do with the stack when each operator is applied on it.
pub fn my_operator_system(
    stack: &mut Stack<MyValue>,
    operator: &MyOperator,
    if_stack: &mut ConditionStack,
    context: &ScriptContext,
) -> Result<(), ScriptError> {
    if !if_stack.all_true() {
        // When execution is disabled, we need to check control flow operators to know when to
        // enable exeuction again.
        return match operator {
            MyOperator::If => if_operator(stack, if_stack),
            MyOperator::Else => else_operator(if_stack),
            MyOperator::EndIf => end_if_operator(if_stack),
            _ => Ok(()),
        };
    }

    match operator {
        MyOperator::Equal => equal_operator(stack),
        MyOperator::Hash160 => hash_160_operator(stack),
        MyOperator::Sha256 => sha_256_operator(stack),
        MyOperator::CheckSig => {
            check_sig_operator(stack, context.tx_hash, context.disable_signature_verify)
        }
        MyOperator::CheckMultiSig => {
            check_multisig_operator(stack, context.tx_hash, context.disable_signature_verify)
        }
        MyOperator::CheckTimeLock => check_timelock_operator(stack, context.block_timestamp),
        MyOperator::Verify => verify_operator(stack),
        MyOperator::If => if_operator(stack, if_stack),
        MyOperator::Else => else_operator(if_stack),
        MyOperator::EndIf => end_if_operator(if_stack),
    }
}
