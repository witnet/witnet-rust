use super::{
    encode, execute_complete_script, execute_locking_script, execute_redeem_script, execute_script,
    Item, MyOperator, MyValue, ScriptContext, ScriptError,
};
use crate::chain::KeyedSignature;
use witnet_crypto::hash::calculate_sha256;

const EQUAL_OPERATOR_HASH: [u8; 20] = [
    52, 128, 191, 80, 253, 28, 169, 253, 237, 29, 0, 51, 201, 0, 31, 203, 157, 99, 218, 210,
];

#[test]
fn test_execute_script() {
    let s = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Integer(10)),
        Item::Operator(MyOperator::Equal),
    ];
    assert!(matches!(
        execute_script(&s, &ScriptContext::default()),
        Ok(true)
    ));

    let s = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Integer(20)),
        Item::Operator(MyOperator::Equal),
    ];
    assert!(matches!(
        execute_script(&s, &ScriptContext::default()),
        Ok(false)
    ));
}

#[test]
fn test_execute_locking_script() {
    let redeem_script = vec![Item::Operator(MyOperator::Equal)];
    let locking_bytes = EQUAL_OPERATOR_HASH;
    assert!(matches!(
        execute_locking_script(
            &encode(&redeem_script).unwrap(),
            &locking_bytes,
            &ScriptContext::default(),
        ),
        Ok(true)
    ));

    let invalid_locking_bytes = [1; 20];
    assert!(matches!(
        execute_locking_script(
            &encode(&redeem_script).unwrap(),
            &invalid_locking_bytes,
            &ScriptContext::default(),
        ),
        Ok(false)
    ));
}

#[test]
fn test_execute_redeem_script() {
    let witness = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Integer(10)),
    ];
    let redeem_script = vec![Item::Operator(MyOperator::Equal)];
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness).unwrap(),
            &encode(&redeem_script).unwrap(),
            &ScriptContext::default(),
        ),
        Ok(true)
    ));

    let invalid_witness = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Integer(20)),
    ];
    assert!(matches!(
        execute_redeem_script(
            &encode(&invalid_witness).unwrap(),
            &encode(&redeem_script).unwrap(),
            &ScriptContext::default(),
        ),
        Ok(false)
    ));
}

#[test]
fn test_complete_script() {
    let witness = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Integer(10)),
    ];
    let redeem_script = vec![Item::Operator(MyOperator::Equal)];
    let locking_bytes = EQUAL_OPERATOR_HASH;
    assert!(matches!(
        execute_complete_script(
            &encode(&witness).unwrap(),
            &encode(&redeem_script).unwrap(),
            &locking_bytes,
            &ScriptContext::default(),
        ),
        Ok(true)
    ));

    let witness = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Integer(20)),
    ];
    assert!(matches!(
        execute_complete_script(
            &encode(&witness).unwrap(),
            &encode(&redeem_script).unwrap(),
            &locking_bytes,
            &ScriptContext::default(),
        ),
        Ok(false)
    ));

    let witness = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Integer(10)),
    ];
    let invalid_locking_bytes: [u8; 20] = [1; 20];
    assert!(matches!(
        execute_complete_script(
            &encode(&witness).unwrap(),
            &encode(&redeem_script).unwrap(),
            &invalid_locking_bytes,
            &ScriptContext::default(),
        ),
        Ok(false)
    ));
}

fn test_ks_id(id: u8) -> KeyedSignature {
    if id == 0 {
        panic!("Invalid secret key");
    }
    let mk = vec![id; 32];
    let secret_key =
        witnet_crypto::secp256k1::SecretKey::from_slice(&mk).expect("32 bytes, within curve order");
    let public_key = witnet_crypto::secp256k1::PublicKey::from_secret_key_global(&secret_key);
    let public_key = crate::chain::PublicKey::from(public_key);
    // TODO: mock this signature, it is not even validated in tests but we need a valid signature to
    // test signature deserialization
    let signature = witnet_crypto::signature::sign(secret_key, &[0x01; 32]).unwrap();

    KeyedSignature {
        signature: signature.into(),
        public_key,
    }
}

#[test]
fn test_check_sig() {
    let ks_1 = test_ks_id(1);
    let ks_2 = test_ks_id(2);

    let pk_1 = ks_1.public_key.clone();

    let witness = vec![Item::Value(MyValue::from_signature(&ks_1))];
    let redeem_bytes = encode(&[
        Item::Value(MyValue::Bytes(pk_1.pkh().bytes().to_vec())),
        Item::Operator(MyOperator::CheckSig),
    ])
    .unwrap();
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness).unwrap(),
            &redeem_bytes,
            &ScriptContext::default_no_signature_verify(),
        ),
        Ok(true)
    ));

    let invalid_witness = vec![Item::Value(MyValue::from_signature(&ks_2))];
    assert!(matches!(
        execute_redeem_script(
            &encode(&invalid_witness).unwrap(),
            &redeem_bytes,
            &ScriptContext::default_no_signature_verify(),
        ),
        Err(ScriptError::WrongSignaturePublicKey)
    ));
}

#[test]
fn test_check_multisig() {
    let ks_1 = test_ks_id(1);
    let ks_2 = test_ks_id(2);
    let ks_3 = test_ks_id(3);

    let pk_1 = ks_1.public_key.clone();
    let pk_2 = ks_2.public_key.clone();
    let pk_3 = ks_3.public_key.clone();

    let witness = vec![
        Item::Value(MyValue::from_signature(&ks_1)),
        Item::Value(MyValue::from_signature(&ks_2)),
    ];
    let redeem_bytes = encode(&[
        Item::Value(MyValue::Integer(2)),
        Item::Value(MyValue::Bytes(pk_1.pkh().bytes().to_vec())),
        Item::Value(MyValue::Bytes(pk_2.pkh().bytes().to_vec())),
        Item::Value(MyValue::Bytes(pk_3.pkh().bytes().to_vec())),
        Item::Value(MyValue::Integer(3)),
        Item::Operator(MyOperator::CheckMultiSig),
    ])
    .unwrap();
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness).unwrap(),
            &redeem_bytes,
            &ScriptContext::default_no_signature_verify(),
        ),
        Ok(true)
    ));

    let other_valid_witness = vec![
        Item::Value(MyValue::from_signature(&ks_1)),
        Item::Value(MyValue::from_signature(&ks_3)),
    ];
    assert!(matches!(
        execute_redeem_script(
            &encode(&other_valid_witness).unwrap(),
            &redeem_bytes,
            &ScriptContext::default_no_signature_verify(),
        ),
        Ok(true)
    ));

    let ks_4 = test_ks_id(4);
    let invalid_witness = vec![
        Item::Value(MyValue::from_signature(&ks_1)),
        Item::Value(MyValue::from_signature(&ks_4)),
    ];
    assert!(matches!(
        execute_redeem_script(
            &encode(&invalid_witness).unwrap(),
            &redeem_bytes,
            &ScriptContext::default_no_signature_verify(),
        ),
        Err(ScriptError::WrongSignaturePublicKey)
    ));
}

#[test]
fn test_execute_script_op_verify() {
    let s = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Integer(10)),
        Item::Operator(MyOperator::Equal),
        Item::Operator(MyOperator::Verify),
        Item::Value(MyValue::Boolean(true)),
    ];
    assert!(matches!(
        execute_script(&s, &ScriptContext::default()),
        Ok(true)
    ));

    let s = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Integer(20)),
        Item::Operator(MyOperator::Equal),
        Item::Operator(MyOperator::Verify),
        Item::Value(MyValue::Boolean(true)),
    ];
    assert!(matches!(
        execute_script(&s, &ScriptContext::default()),
        Err(ScriptError::VerifyOpFailed)
    ));

    let s = vec![
        Item::Operator(MyOperator::Verify),
        Item::Value(MyValue::Boolean(true)),
    ];
    assert!(matches!(
        execute_script(&s, &ScriptContext::default()),
        Err(ScriptError::EmptyStackPop)
    ));
}

#[test]
fn test_execute_script_op_if() {
    let s = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Boolean(true)),
        Item::Operator(MyOperator::If),
        Item::Value(MyValue::Integer(10)),
        Item::Operator(MyOperator::Else),
        Item::Value(MyValue::Integer(20)),
        Item::Operator(MyOperator::EndIf),
        Item::Operator(MyOperator::Equal),
    ];
    assert!(matches!(
        execute_script(&s, &ScriptContext::default()),
        Ok(true)
    ));

    let s = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Boolean(false)),
        Item::Operator(MyOperator::If),
        Item::Value(MyValue::Integer(10)),
        Item::Operator(MyOperator::Else),
        Item::Value(MyValue::Integer(20)),
        Item::Operator(MyOperator::EndIf),
        Item::Operator(MyOperator::Equal),
    ];
    assert!(matches!(
        execute_script(&s, &ScriptContext::default()),
        Ok(false)
    ));
}

#[test]
fn test_execute_script_op_if_nested() {
    let s = vec![
        Item::Value(MyValue::Integer(10)),
        Item::Value(MyValue::Boolean(true)),
        Item::Operator(MyOperator::If),
        Item::Value(MyValue::Integer(10)),
        Item::Operator(MyOperator::Else),
        Item::Value(MyValue::Boolean(false)),
        Item::Operator(MyOperator::If),
        Item::Value(MyValue::Integer(10)),
        Item::Operator(MyOperator::Else),
        Item::Value(MyValue::Integer(20)),
        Item::Operator(MyOperator::EndIf),
        Item::Operator(MyOperator::EndIf),
        Item::Operator(MyOperator::Equal),
    ];
    assert!(matches!(
        execute_script(&s, &ScriptContext::default()),
        Ok(true)
    ));

    let s = vec![
        Item::Value(MyValue::Integer(20)),
        Item::Value(MyValue::Boolean(false)),
        Item::Operator(MyOperator::If),
        Item::Value(MyValue::Integer(10)),
        Item::Operator(MyOperator::Else),
        Item::Value(MyValue::Boolean(false)),
        Item::Operator(MyOperator::If),
        Item::Value(MyValue::Integer(10)),
        Item::Operator(MyOperator::Else),
        Item::Value(MyValue::Integer(20)),
        Item::Operator(MyOperator::EndIf),
        Item::Operator(MyOperator::EndIf),
        Item::Operator(MyOperator::Equal),
    ];
    assert!(matches!(
        execute_script(&s, &ScriptContext::default()),
        Ok(true)
    ));
}

#[test]
fn test_execute_script_op_check_timelock() {
    let s = vec![
        Item::Value(MyValue::Integer(10_000)),
        Item::Operator(MyOperator::CheckTimeLock),
    ];
    assert!(matches!(
        execute_script(
            &s,
            &ScriptContext {
                block_timestamp: 20_000,
                ..Default::default()
            }
        ),
        Ok(true)
    ));
    assert!(matches!(
        execute_script(
            &s,
            &ScriptContext {
                block_timestamp: 0,
                ..Default::default()
            }
        ),
        Ok(false)
    ));
}

#[test]
fn test_execute_script_atomic_swap() {
    let secret = vec![1, 2, 3, 4];
    let hash_secret = calculate_sha256(&secret);
    let ks_1 = test_ks_id(1);
    let ks_2 = test_ks_id(2);
    let pk_1 = ks_1.public_key.clone();
    let pk_2 = ks_2.public_key.clone();

    let redeem_bytes = encode(&[
        Item::Operator(MyOperator::If),
        Item::Value(MyValue::Integer(10_000)),
        Item::Operator(MyOperator::CheckTimeLock),
        Item::Operator(MyOperator::Verify),
        Item::Value(MyValue::Bytes(pk_1.pkh().bytes().to_vec())),
        Item::Operator(MyOperator::CheckSig),
        Item::Operator(MyOperator::Else),
        Item::Operator(MyOperator::Sha256),
        Item::Value(MyValue::Bytes(hash_secret.as_ref().to_vec())),
        Item::Operator(MyOperator::Equal),
        Item::Operator(MyOperator::Verify),
        Item::Value(MyValue::Bytes(pk_2.pkh().bytes().to_vec())),
        Item::Operator(MyOperator::CheckSig),
        Item::Operator(MyOperator::EndIf),
    ])
    .unwrap();

    // 1 can spend after timelock
    let witness_script = vec![
        Item::Value(MyValue::from_signature(&ks_1)),
        Item::Value(MyValue::Boolean(true)),
    ];
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness_script).unwrap(),
            &redeem_bytes,
            &ScriptContext {
                block_timestamp: 20_000,
                ..ScriptContext::default_no_signature_verify()
            }
        ),
        Ok(true),
    ));

    // 1 cannot spend before timelock
    let witness_script = vec![
        Item::Value(MyValue::from_signature(&ks_1)),
        Item::Value(MyValue::Boolean(true)),
    ];
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness_script).unwrap(),
            &redeem_bytes,
            &ScriptContext {
                block_timestamp: 0,
                ..ScriptContext::default_no_signature_verify()
            }
        ),
        Err(ScriptError::VerifyOpFailed)
    ));

    // 2 can spend with secret
    let witness_script = vec![
        Item::Value(MyValue::from_signature(&ks_2)),
        Item::Value(MyValue::Bytes(secret)),
        Item::Value(MyValue::Boolean(false)),
    ];
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness_script).unwrap(),
            &redeem_bytes,
            &ScriptContext {
                block_timestamp: 0,
                ..ScriptContext::default_no_signature_verify()
            }
        ),
        Ok(true)
    ));

    // 2 cannot spend with a wrong secret
    let witness_script = vec![
        Item::Value(MyValue::from_signature(&ks_2)),
        Item::Value(MyValue::Bytes(vec![0, 0, 0, 0])),
        Item::Value(MyValue::Boolean(false)),
    ];
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness_script).unwrap(),
            &redeem_bytes,
            &ScriptContext {
                block_timestamp: 0,
                ..ScriptContext::default_no_signature_verify()
            }
        ),
        Err(ScriptError::VerifyOpFailed)
    ));
}

#[test]
fn test_execute_script_atomic_swap_2() {
    let secret = vec![1, 2, 3, 4];
    let hash_secret = calculate_sha256(&secret);
    let ks_1 = test_ks_id(1);
    let ks_2 = test_ks_id(2);
    let pk_1 = ks_1.public_key.clone();
    let pk_2 = ks_2.public_key.clone();

    let redeem_bytes = encode(&[
        Item::Value(MyValue::Integer(10_000)),
        Item::Operator(MyOperator::CheckTimeLock),
        Item::Operator(MyOperator::If),
        Item::Value(MyValue::Bytes(pk_1.pkh().bytes().to_vec())),
        Item::Operator(MyOperator::CheckSig),
        Item::Operator(MyOperator::Else),
        Item::Operator(MyOperator::Sha256),
        Item::Value(MyValue::Bytes(hash_secret.as_ref().to_vec())),
        Item::Operator(MyOperator::Equal),
        Item::Operator(MyOperator::Verify),
        Item::Value(MyValue::Bytes(pk_2.pkh().bytes().to_vec())),
        Item::Operator(MyOperator::CheckSig),
        Item::Operator(MyOperator::EndIf),
    ])
    .unwrap();

    // 1 can spend after timelock
    let witness_script = vec![Item::Value(MyValue::from_signature(&ks_1))];
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness_script).unwrap(),
            &redeem_bytes,
            &ScriptContext {
                block_timestamp: 20_000,
                ..ScriptContext::default_no_signature_verify()
            }
        ),
        Ok(true)
    ));
    // 1 cannot spend before timelock
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness_script).unwrap(),
            &redeem_bytes,
            &ScriptContext {
                block_timestamp: 0,
                ..ScriptContext::default_no_signature_verify()
            }
        ),
        Err(ScriptError::VerifyOpFailed)
    ));

    // 2 can spend with secret
    let witness_script = vec![
        Item::Value(MyValue::from_signature(&ks_2)),
        Item::Value(MyValue::Bytes(secret.clone())),
    ];
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness_script).unwrap(),
            &redeem_bytes,
            &ScriptContext {
                block_timestamp: 0,
                ..ScriptContext::default_no_signature_verify()
            }
        ),
        Ok(true)
    ));

    // 2 cannot spend with a wrong secret
    let witness_script = vec![
        Item::Value(MyValue::from_signature(&ks_2)),
        Item::Value(MyValue::Bytes(vec![0, 0, 0, 0])),
    ];
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness_script).unwrap(),
            &redeem_bytes,
            &ScriptContext {
                block_timestamp: 0,
                ..ScriptContext::default_no_signature_verify()
            }
        ),
        Err(ScriptError::VerifyOpFailed)
    ));

    // 2 cannot spend after timelock
    let witness_script = vec![
        Item::Value(MyValue::from_signature(&ks_2)),
        Item::Value(MyValue::Bytes(secret)),
    ];
    assert!(matches!(
        execute_redeem_script(
            &encode(&witness_script).unwrap(),
            &redeem_bytes,
            &ScriptContext {
                block_timestamp: 20_000,
                ..ScriptContext::default_no_signature_verify()
            }
        ),
        Err(ScriptError::InvalidSignature)
    ));
}
