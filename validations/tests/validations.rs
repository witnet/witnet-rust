use std::sync::atomic::{AtomicU32, Ordering};
use witnet_crypto::{
    secp256k1::{PublicKey as Secp256k1_PublicKey, Secp256k1, SecretKey as Secp256k1_SecretKey},
    signature::sign,
};
use witnet_data_structures::{
    chain::*,
    data_request::DataRequestPool,
    error::{BlockError, DataRequestError, Secp256k1ConversionError, TransactionError},
    transaction::*,
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim, VrfCtx},
};
use witnet_rad::error::RadError;
use witnet_validations::validations::*;

static MY_PKH: &str = "3e13996ed18be842d9d4303b428dd30c85db8e9e";
static MY_PKH_2: &str = "11f66b6ff5ed1ed21200c0a01f22540cd828ca1f";

fn sign_t<H: Hashable>(tx: &H) -> KeyedSignature {
    let Hash::SHA256(data) = tx.hash();

    let secp = Secp256k1::new();
    let secret_key =
        Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
    let public_key = Secp256k1_PublicKey::from_secret_key(&secp, &secret_key);
    let public_key = PublicKey::from(public_key);
    assert_eq!(public_key.pkh(), MY_PKH.parse().unwrap());

    let signature = sign(secret_key, &data);

    KeyedSignature {
        signature: Signature::from(signature),
        public_key,
    }
}

// Sign with a different public key
fn sign_t2<H: Hashable>(tx: &H) -> KeyedSignature {
    let Hash::SHA256(data) = tx.hash();

    let secp = Secp256k1::new();
    let secret_key =
        Secp256k1_SecretKey::from_slice(&[0x43; 32]).expect("32 bytes, within curve order");
    let public_key = Secp256k1_PublicKey::from_secret_key(&secp, &secret_key);
    let public_key = PublicKey::from(public_key);
    assert_eq!(public_key.pkh(), MY_PKH_2.parse().unwrap());

    let signature = sign(secret_key, &data);

    KeyedSignature {
        signature: Signature::from(signature),
        public_key,
    }
}

// Counter used to prevent creating two transactions with the same hash
static TX_COUNTER: AtomicU32 = AtomicU32::new(0);

fn build_utxo_set_with_mint<T: Into<Option<UnspentOutputsPool>>>(
    minted_outputs: Vec<ValueTransferOutput>,
    all_utxos: T,
    mut txns: Vec<Transaction>,
) -> UnspentOutputsPool {
    txns.extend(minted_outputs.into_iter().map(|o| {
        Transaction::Mint(MintTransaction::new(
            TX_COUNTER.fetch_add(1, Ordering::SeqCst),
            o,
        ))
    }));

    let all_utxos = all_utxos.into().unwrap_or_default();

    generate_unspent_outputs_pool(&all_utxos, &txns)
}

// Validate transactions in block
#[test]
fn mint_mismatched_reward() {
    let epoch = 0;
    let total_fees = 100;
    let reward = block_reward(epoch);
    // Build mint without the block reward
    let output = ValueTransferOutput {
        pkh: Default::default(),
        value: 100,
    };
    let mint_tx = MintTransaction::new(epoch, output);
    let x = validate_mint_transaction(&mint_tx, total_fees, epoch);
    // Error: block reward mismatch
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::MismatchedMintValue {
            mint_value: 100,
            fees_value: 100,
            reward_value: reward,
        }
    );
}

#[test]
fn mint_invalid_epoch() {
    let epoch = 0;
    let reward = block_reward(epoch);
    let total_fees = 100;
    let output = ValueTransferOutput {
        pkh: Default::default(),
        value: reward + total_fees,
    };
    // Build a mint for the next epoch
    let mint_tx = MintTransaction::new(epoch + 1, output);
    let x = validate_mint_transaction(&mint_tx, total_fees, epoch);
    // Error: invalid mint epoch
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::InvalidMintEpoch {
            mint_epoch: 1,
            block_epoch: 0,
        }
    );
}

#[test]
fn mint_valid() {
    let epoch = 0;
    let reward = block_reward(epoch);
    let total_fees = 100;
    let output = ValueTransferOutput {
        pkh: Default::default(),
        value: total_fees + reward,
    };
    let mint_tx = MintTransaction::new(epoch, output);
    let x = validate_mint_transaction(&mint_tx, total_fees, epoch);
    assert_eq!(x.unwrap(), ());
}

#[test]
fn vtt_no_inputs_no_outputs() {
    let utxo_pool = UnspentOutputsPool::default();
    let utxo_diff = UtxoDiff::new(&utxo_pool);

    let vt_body = VTTransactionBody::new(vec![], vec![]);
    let vt_tx = VTTransaction::new(vt_body, vec![]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NoInputs {
            tx_hash: vt_tx.hash(),
        }
    );
}

#[test]
fn vtt_no_inputs_zero_output() {
    let utxo_pool = UnspentOutputsPool::default();
    let utxo_diff = UtxoDiff::new(&utxo_pool);

    // Try to create a data request with no inputs
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput { pkh, value: 0 };

    let vt_body = VTTransactionBody::new(vec![], vec![vto0]);
    let vt_tx = VTTransaction::new(vt_body, vec![]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NoInputs {
            tx_hash: vt_tx.hash(),
        }
    );
}

#[test]
fn vtt_no_inputs() {
    let utxo_pool = UnspentOutputsPool::default();
    let utxo_diff = UtxoDiff::new(&utxo_pool);

    // Try to create a data request with no inputs
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput { pkh, value: 1000 };

    let vt_body = VTTransactionBody::new(vec![], vec![vto0]);
    let vt_tx = VTTransaction::new(vt_body, vec![]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NoInputs {
            tx_hash: vt_tx.hash(),
        }
    );
}

#[test]
fn vtt_no_inputs_but_one_signature() {
    let utxo_pool = UnspentOutputsPool::default();
    let utxo_diff = UtxoDiff::new(&utxo_pool);

    // No inputs but 1 signature
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput { pkh, value: 1000 };

    let vt_body = VTTransactionBody::new(vec![], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchingSignaturesNumber {
            signatures_n: 1,
            inputs_n: 0,
        }
    );
}

#[test]
fn vtt_one_input_but_no_signature() {
    let utxo_pool = UnspentOutputsPool::default();
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(
        "2222222222222222222222222222222222222222222222222222222222222222:0"
            .parse()
            .unwrap(),
    );

    // No signatures but 1 input
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput { pkh, value: 1000 };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
    let vt_tx = VTTransaction::new(vt_body, vec![]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchingSignaturesNumber {
            signatures_n: 0,
            inputs_n: 1,
        }
    );
}

fn test_signature_empty_wrong_bad<F, H>(hashable: H, mut f: F)
where
    F: FnMut(H, KeyedSignature) -> Result<(), failure::Error>,
    H: Hashable + Clone,
{
    let ks = sign_t(&hashable);
    let hash = hashable.hash();

    // Replace the signature with default (all zeros)
    let ks_default = KeyedSignature::default();
    let signature_pkh = ks_default.public_key.pkh();
    let x = f(hashable.clone(), ks_default);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: TransactionError::PublicKeyHashMismatch {
                expected_pkh: MY_PKH.parse().unwrap(),
                signature_pkh,
            }
            .to_string()
        },
    );

    // Replace the signature with an empty vector
    let mut ks_empty = ks.clone();
    match ks_empty.signature {
        Signature::Secp256k1(ref mut x) => x.der = vec![],
    }
    let x = f(hashable.clone(), ks_empty);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: Secp256k1ConversionError::FailSignatureConversion.to_string(),
        },
    );

    // Flip one bit in the signature
    let mut ks_wrong = ks.clone();
    match ks_wrong.signature {
        Signature::Secp256k1(ref mut x) => x.der[10] ^= 0x1,
    }
    let x = f(hashable.clone(), ks_wrong);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: "Fail in verify process".to_string(),
        },
    );

    // Flip one bit in the public key of the signature
    let mut ks_bad_pk = ks.clone();
    ks_bad_pk.public_key.bytes[13] ^= 0x01;
    let signature_pkh = ks_bad_pk.public_key.pkh();
    let x = f(hashable.clone(), ks_bad_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            // A "Fail in verify process" msg would also be correct here
            msg: TransactionError::PublicKeyHashMismatch {
                expected_pkh: MY_PKH.parse().unwrap(),
                signature_pkh,
            }
            .to_string(),
        }
    );

    // Sign transaction with a different public key
    let ks_different_pk = sign_t2(&hashable);
    let signature_pkh = ks_different_pk.public_key.pkh();
    let x = f(hashable.clone(), ks_different_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: TransactionError::PublicKeyHashMismatch {
                expected_pkh: MY_PKH.parse().unwrap(),
                signature_pkh,
            }
            .to_string(),
        }
    );
}

#[test]
fn vtt_one_input_signatures() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput { pkh, value: 1000 };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);

    test_signature_empty_wrong_bad(vt_body, |vt_body, vts| {
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);

        validate_vt_transaction(&vt_tx, &utxo_diff).map(|_| ())
    });
}

#[test]
fn vtt_input_not_in_utxo() {
    let utxo_pool = UnspentOutputsPool::default();
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(
        "2222222222222222222222222222222222222222222222222222222222222222:0"
            .parse()
            .unwrap(),
    );

    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput { pkh, value: 1000 };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::OutputNotFound {
            output: "2222222222222222222222222222222222222222222222222222222222222222:0"
                .parse()
                .unwrap(),
        }
    );
}

#[test]
fn vtt_input_not_enough_value() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput { pkh, value: 1000 };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee
    );
}

#[test]
fn vtt_one_input_zero_value_output() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let zero_output = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![zero_output]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::ZeroValueOutput {
            tx_hash: vt_tx.hash(),
            output_id: 0,
        }
    );
}

#[test]
fn vtt_one_input_two_outputs_negative_fee() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 2,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 2,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0, vto1]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee,
    );
}

#[test]
fn vtt_one_input_two_outputs() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 21,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 13,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 7,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0, vto1]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff).map(|(_, _, fee)| fee);
    assert_eq!(x.unwrap(), 21 - 13 - 7,);
}

#[test]
fn vtt_two_inputs_one_signature() {
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 21,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 13,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti0 = Input::new(utxo_pool.iter().nth(0).unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 10,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchingSignaturesNumber {
            signatures_n: 1,
            inputs_n: 2,
        }
    );
}

#[test]
fn vtt_two_inputs_one_signature_wrong_pkh() {
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 21,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 13,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti0 = Input::new(utxo_pool.iter().nth(0).unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 10,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vts2 = sign_t2(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts, vts2]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash: vt_tx.hash(),
            index: 1,
            msg: TransactionError::PublicKeyHashMismatch {
                expected_pkh: MY_PKH.parse().unwrap(),
                signature_pkh: MY_PKH_2.parse().unwrap(),
            }
            .to_string(),
        }
    );
}

#[test]
fn vtt_two_inputs_three_signatures() {
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 21,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 13,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti0 = Input::new(utxo_pool.iter().nth(0).unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 10,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts.clone(), vts.clone(), vts]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchingSignaturesNumber {
            signatures_n: 3,
            inputs_n: 2,
        }
    );
}

#[test]
fn vtt_two_inputs_two_outputs() {
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 21,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 13,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti0 = Input::new(utxo_pool.iter().nth(0).unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 10,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 20,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0, vto1]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts.clone(), vts]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff).map(|(_, _, fee)| fee);
    assert_eq!(x.unwrap(), 21 + 13 - 10 - 20,);
}

#[test]
fn vtt_valid() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput { pkh, value: 1000 };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(&vt_tx, &utxo_diff).map(|(_, _, fee)| fee);
    // The fee is 1000 - 1000 = 0
    assert_eq!(x.unwrap(), 0,);
}

#[test]
fn data_request_no_inputs() {
    let utxo_pool = UnspentOutputsPool::default();
    let utxo_diff = UtxoDiff::new(&utxo_pool);

    // Try to create a data request with no inputs
    let dr_output = DataRequestOutput {
        value: 1000,
        witnesses: 2,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![], vec![], dr_output);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![]);
    let x = validate_dr_transaction(&dr_transaction, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee
    );
}

#[test]
fn data_request_no_inputs_but_one_signature() {
    let utxo_pool = UnspentOutputsPool::default();
    let utxo_diff = UtxoDiff::new(&utxo_pool);

    // No inputs but 1 signature
    let dr_output = DataRequestOutput {
        value: 1000,
        witnesses: 2,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![], vec![], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);
    let x = validate_dr_transaction(&dr_transaction, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchingSignaturesNumber {
            signatures_n: 1,
            inputs_n: 0,
        }
    );
}

#[test]
fn data_request_one_input_but_no_signature() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let dr_output = DataRequestOutput {
        value: 1000,
        witnesses: 2,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);

    let dr_transaction = DRTransaction::new(dr_tx_body, vec![]);

    let x = validate_dr_transaction(&dr_transaction, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchingSignaturesNumber {
            inputs_n: 1,
            signatures_n: 0,
        }
    );
}

#[test]
fn data_request_one_input_signatures() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let dr_output = DataRequestOutput {
        value: 1000,
        witnesses: 2,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);

    test_signature_empty_wrong_bad(dr_tx_body, |dr_tx_body, drs| {
        let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

        validate_dr_transaction(&dr_transaction, &utxo_diff).map(|_| ())
    });
}

#[test]
fn data_request_input_not_in_utxo() {
    let utxo_pool = UnspentOutputsPool::default();
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(
        "2222222222222222222222222222222222222222222222222222222222222222:0"
            .parse()
            .unwrap(),
    );

    let dr_output = DataRequestOutput {
        value: 1000,
        witnesses: 2,
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);
    let x = validate_dr_transaction(&dr_transaction, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::OutputNotFound {
            output: "2222222222222222222222222222222222222222222222222222222222222222:0"
                .parse()
                .unwrap(),
        }
    );
}

#[test]
fn data_request_input_not_enough_value() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let dr_output = DataRequestOutput {
        value: 1000,
        witnesses: 2,
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);
    let x = validate_dr_transaction(&dr_transaction, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee
    );
}

// Helper function which creates a data request with a valid input with value 1000
// and returns the validation error
fn test_drtx(dr_output: DataRequestOutput) -> Result<(), failure::Error> {
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    validate_dr_transaction(&dr_transaction, &utxo_diff).map(|_| ())
}

fn test_rad_request(data_request: RADRequest) -> Result<(), failure::Error> {
    test_drtx(DataRequestOutput {
        value: 1000,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    })
}

// Example data request used in tests. It consists of just empty arrays.
// If this data request is modified, the `data_request_empty_scripts` test
// should be updated accordingly.
fn example_data_request() -> RADRequest {
    RADRequest {
        not_before: 0,
        retrieve: vec![RADRetrieve {
            kind: RADType::HttpGet,
            url: "".to_string(),
            script: vec![0x90],
        }],
        aggregate: RADAggregate { script: vec![0x90] },
        consensus: RADConsensus { script: vec![0x90] },
        deliver: vec![RADDeliver {
            kind: RADType::HttpGet,
            url: "".to_string(),
        }],
    }
}

#[test]
fn data_request_no_scripts() {
    let x = test_rad_request(RADRequest {
        not_before: 0,
        retrieve: vec![],
        aggregate: RADAggregate { script: vec![] },
        consensus: RADConsensus { script: vec![] },
        deliver: vec![],
    });
    assert_eq!(
        x.unwrap_err().downcast::<RadError>().unwrap(),
        // This should be a RadError, but we probably cannot import it here
        // unless we add witnet_rad to Cargo.toml.
        // Anyway, these types of tests belong to witnet_rad.
        //"Failed to parse a Value from a MessagePack buffer. Error message: I/O error while reading marker byte",
        RadError::MessagePack {
            description: "I/O error while reading marker byte".to_string(),
        }
    );
}

#[test]
fn data_request_empty_scripts() {
    // 0x90 is an empty array in MessagePack
    let x = test_rad_request(example_data_request());

    // This is currently accepted as a valid data request.
    // If this test fails in the future, modify it to check that
    // this is an invalid data request.
    assert_eq!(x.unwrap(), ());
}

#[test]
fn data_request_witnesses_0() {
    // A data request with 0 witnesses is invalid
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        witnesses: 0,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InsufficientWitnesses,
    );
}

#[test]
fn data_request_witnesses_1() {
    // A data request with 1 witness is currently accepted
    // But that can change in the future
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        witnesses: 1,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(x.unwrap(), ());
}

#[test]
fn data_request_no_value() {
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 0,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestReward { reward: 0 },
    );
}

#[test]
fn data_request_odd_value() {
    // A data request with 2 witnesses must have an even value,
    // because it will be divided between the 2 witnesses.
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 999,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestValue {
            dr_value: 999,
            witnesses: 2,
        }
    );
}

#[test]
fn data_request_odd_commit_value() {
    // A data request with 2 witnesses must have an even value,
    // because it will be divided between the 2 witnesses.
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        commit_fee: 901,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestValue {
            dr_value: 99,
            witnesses: 2,
        }
    );
}

#[test]
fn data_request_odd_reveal_value() {
    // A data request with 2 witnesses must have an even value,
    // because it will be divided between the 2 witnesses.
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        reveal_fee: 901,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestValue {
            dr_value: 99,
            witnesses: 2,
        }
    );
}

#[test]
fn data_request_odd_tally_value() {
    // A data request with 2 witnesses must have an even value,
    // because it will be divided between the 2 witnesses.
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        tally_fee: 901,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestValue {
            dr_value: 99,
            witnesses: 2,
        }
    );
}

#[test]
fn data_request_invalid_value_commit_fee() {
    // 1000 - 1000 = 0, so the witnesses get 0 reward
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        commit_fee: 1000,
        reveal_fee: 0,
        tally_fee: 0,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestReward { reward: 0 },
    );
}

#[test]
fn data_request_invalid_value_reveal_fee() {
    // 1000 - 1000 = 0, so the witnesses get 0 reward
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        commit_fee: 0,
        reveal_fee: 1000,
        tally_fee: 0,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestReward { reward: 0 },
    );
}

#[test]
fn data_request_invalid_value_tally_fee() {
    // 1000 - 1000 = 0, so the witnesses get 0 reward
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        commit_fee: 0,
        reveal_fee: 0,
        tally_fee: 1000,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestReward { reward: 0 },
    );
}

#[test]
fn data_request_invalid_all_fees() {
    // 1000 - 250 - 250 - 500 = 0, so the witnesses get 0 reward
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        commit_fee: 250,
        reveal_fee: 250,
        tally_fee: 500,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestReward { reward: 0 },
    );
}

#[test]
fn data_request_negative_value_commit_fee() {
    // 1000 - 1000 = 0, so the witnesses get 0 reward
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        commit_fee: 1001,
        reveal_fee: 0,
        tally_fee: 0,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestReward { reward: -1 },
    );
}

#[test]
fn data_request_negative_value_reveal_fee() {
    // 1000 - 1000 = 0, so the witnesses get 0 reward
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        commit_fee: 0,
        reveal_fee: 1001,
        tally_fee: 0,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestReward { reward: -1 },
    );
}

#[test]
fn data_request_negative_value_tally_fee() {
    // 1000 - 1001 = 0, so the witnesses get 0 reward
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        commit_fee: 0,
        reveal_fee: 0,
        tally_fee: 1001,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestReward { reward: -1 },
    );
}

#[test]
fn data_request_negative_all_fees() {
    // 1000 - 251 - 251 - 502 = -4, so the witnesses get 0 reward
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        value: 1000,
        commit_fee: 251,
        reveal_fee: 251,
        tally_fee: 502,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestReward { reward: -4 },
    );
}

#[test]
fn data_request_miner_fee() {
    // Use 1000 input to pay 750 for data request
    let data_request = example_data_request();
    let dr_output = DataRequestOutput {
        value: 750,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    };

    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let dr_miner_fee = validate_dr_transaction(&dr_transaction, &utxo_diff)
        .map(|(_, _, fee)| fee)
        .unwrap();
    assert_eq!(dr_miner_fee, 1000 - 750);
}

#[test]
fn data_request_miner_fee_with_change() {
    // Use 1000 input to pay 750 for data request, and request 200 change (+50 fee)
    let data_request = example_data_request();
    let dr_output = DataRequestOutput {
        value: 750,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    };

    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let change_output = ValueTransferOutput {
        pkh: PublicKeyHash::default(),
        value: 200,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![change_output], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let dr_miner_fee = validate_dr_transaction(&dr_transaction, &utxo_diff)
        .map(|(_, _, fee)| fee)
        .unwrap();
    assert_eq!(dr_miner_fee, 1000 - 750 - 200);
}

#[test]
fn data_request_miner_fee_with_too_much_change() {
    // Use 1000 input to pay 750 for data request, and request 300 change (-50 fee)
    let data_request = example_data_request();
    let dr_output = DataRequestOutput {
        value: 750,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    };

    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let change_output = ValueTransferOutput {
        pkh: PublicKeyHash::default(),
        value: 300,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![change_output], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let x = validate_dr_transaction(&dr_transaction, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee
    );
}

#[test]
fn data_request_zero_value_output() {
    // Use 1000 input to pay 750 for data request, and request 300 change (-50 fee)
    let data_request = example_data_request();
    let dr_output = DataRequestOutput {
        value: 750,
        witnesses: 2,
        data_request,
        ..DataRequestOutput::default()
    };

    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let change_output = ValueTransferOutput {
        pkh: PublicKeyHash::default(),
        value: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_pool);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![change_output], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let x = validate_dr_transaction(&dr_transaction, &utxo_diff);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::ZeroValueOutput {
            tx_hash: dr_transaction.hash(),
            output_id: 0,
        }
    );
}

// Helper function to test a commit with an empty state (no utxos, no drs, etc)
fn test_empty_commit(c_tx: &CommitTransaction) -> Result<(), failure::Error> {
    let dr_pool = DataRequestPool::default();
    let beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);

    validate_commit_transaction(&c_tx, &dr_pool, beacon, vrf, &rep_eng).map(|_| ())
}

static DR_HASH: &str = "d58a420a3e219ec50ab73fb59df7377dc23fb8e9a872956960f3d6f4da485784";

// Helper function to test a commit with an empty state (no utxos, no drs, etc)
fn test_commit_with_dr(c_tx: &CommitTransaction) -> Result<(), failure::Error> {
    let mut dr_pool = DataRequestPool::default();
    let commit_beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);

    let dro = DataRequestOutput {
        value: 1000,
        witnesses: 1,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    assert_eq!(dr_hash, DR_HASH.parse().unwrap());
    let dr_epoch = 0;
    dr_pool.process_data_request(&dr_transaction, dr_epoch);

    validate_commit_transaction(&c_tx, &dr_pool, commit_beacon, vrf, &rep_eng).map(|_| ())
}

// Helper function to test a commit with an existing data request,
// but it is very difficult to construct a valid vrf proof
fn test_commit_difficult_proof() -> Result<(), failure::Error> {
    let mut dr_pool = DataRequestPool::default();
    let commit_beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let secret_key = SecretKey { bytes: [0xcd; 32] };

    // Create a reputation engine where one identity has 1_023 reputation,
    // so it is very difficult for someone with 0 reputation to be elegible
    // for a data request
    let mut rep_eng = ReputationEngine::new(100);
    let rep_pkh = PublicKeyHash::default();
    rep_eng
        .trs
        .gain(Alpha(1000), vec![(rep_pkh, Reputation(1_023))])
        .unwrap();
    rep_eng.ars.push_activity(vec![rep_pkh]);

    let dro = DataRequestOutput {
        value: 1000,
        witnesses: 1,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    assert_eq!(dr_hash, DR_HASH.parse().unwrap());
    let dr_epoch = 0;
    dr_pool.process_data_request(&dr_transaction, dr_epoch);

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof =
        DataRequestEligibilityClaim::create(vrf, &secret_key, commit_beacon, dr_hash).unwrap();
    // Sign commitment
    let cs = sign_t(&cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    validate_commit_transaction(&c_tx, &dr_pool, commit_beacon, vrf, &rep_eng).map(|_| ())
}

// Helper function to test a commit with an existing data request
fn test_commit() -> Result<(), failure::Error> {
    let mut dr_pool = DataRequestPool::default();
    let commit_beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let secret_key = SecretKey { bytes: [0xcd; 32] };

    let dro = DataRequestOutput {
        value: 1000,
        witnesses: 1,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    assert_eq!(dr_hash, DR_HASH.parse().unwrap());
    let dr_epoch = 0;
    dr_pool.process_data_request(&dr_transaction, dr_epoch);

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof =
        DataRequestEligibilityClaim::create(vrf, &secret_key, commit_beacon, dr_hash).unwrap();
    // Sign commitment
    let cs = sign_t(&cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    validate_commit_transaction(&c_tx, &dr_pool, commit_beacon, vrf, &rep_eng).map(|_| ())
}

#[test]
fn commitment_signatures() {
    let dr_hash = DR_HASH.parse().unwrap();
    let commit_beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let secret_key = SecretKey { bytes: [0xcd; 32] };
    let mut cb = CommitTransactionBody::default();
    // Insert valid proof
    cb.dr_pointer = dr_hash;
    cb.proof =
        DataRequestEligibilityClaim::create(vrf, &secret_key, commit_beacon, dr_hash).unwrap();
    let f = |cb, cs| {
        let c_tx = CommitTransaction::new(cb, vec![cs]);

        test_commit_with_dr(&c_tx)
    };

    let hashable = cb;

    let ks = sign_t(&hashable);
    let hash = hashable.hash();

    // Replace the signature with default (all zeros)
    let ks_default = KeyedSignature::default();
    let x = f(hashable.clone(), ks_default);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: Secp256k1ConversionError::FailSignatureConversion.to_string(),
        },
    );

    // Replace the signature with an empty vector
    let mut ks_empty = ks.clone();
    match ks_empty.signature {
        Signature::Secp256k1(ref mut x) => x.der = vec![],
    }
    let x = f(hashable.clone(), ks_empty);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: Secp256k1ConversionError::FailSignatureConversion.to_string(),
        },
    );

    // Flip one bit in the signature
    let mut ks_wrong = ks.clone();
    match ks_wrong.signature {
        Signature::Secp256k1(ref mut x) => x.der[10] ^= 0x1,
    }
    let x = f(hashable.clone(), ks_wrong);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: "Fail in verify process".to_string(),
        },
    );

    // Flip one bit in the public key of the signature
    let mut ks_bad_pk = ks.clone();
    ks_bad_pk.public_key.bytes[13] ^= 0x01;
    let x = f(hashable.clone(), ks_bad_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: "Fail in verify process".to_string(),
        }
    );

    // Sign transaction with a different public key
    let ks_different_pk = sign_t2(&hashable);
    let signature_pkh = ks_different_pk.public_key.pkh();
    let x = f(hashable.clone(), ks_different_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::PublicKeyHashMismatch {
            expected_pkh: MY_PKH.parse().unwrap(),
            signature_pkh,
        }
    );
}

#[test]
fn commitment_no_signature() {
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = DR_HASH.parse().unwrap();
    let c_tx = CommitTransaction::new(cb, vec![]);

    let x = test_commit_with_dr(&c_tx);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::SignatureNotFound,
    );
}

#[test]
fn commitment_unknown_dr() {
    let dr_pointer = "2222222222222222222222222222222222222222222222222222222222222222"
        .parse()
        .unwrap();
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_pointer;
    let cs = sign_t(&cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    let x = test_empty_commit(&c_tx);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DataRequestNotFound { hash: dr_pointer },
    );
}

#[test]
fn commitment_invalid_proof() {
    let dr_pointer = DR_HASH.parse().unwrap();
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_pointer;

    let mut dr_pool = DataRequestPool::default();
    let commit_beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let secret_key = SecretKey { bytes: [0xcd; 32] };

    // Create an invalid proof by suppliying a different dr_pointer
    let bad_dr_pointer = Hash::default();
    cb.proof = DataRequestEligibilityClaim::create(vrf, &secret_key, commit_beacon, bad_dr_pointer)
        .unwrap();

    let dro = DataRequestOutput {
        value: 1000,
        witnesses: 1,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_epoch = 0;
    dr_pool.process_data_request(&dr_transaction, dr_epoch);

    // Sign commitment
    let cs = sign_t(&cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    let x = validate_commit_transaction(&c_tx, &dr_pool, commit_beacon, vrf, &rep_eng).map(|_| ());

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestPoe,
    );
}

#[test]
fn commitment_proof_lower_than_target() {
    let x = test_commit_difficult_proof();
    // This is just the hash of the VRF, we do not care for the exact value as
    // long as it is below the target hash
    let vrf_hash = "ee24cf23f163d951f1d803451397fdaa05edc247bf3fa4d4b84fcdfe925a6072"
        .parse()
        .unwrap();
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DataRequestEligibilityDoesNotMeetTarget {
            vrf_hash,
            target_hash: Hash::with_first_u32(0x003f_ffff),
        }
    );
}

#[test]
fn commitment_dr_in_reveal_stage() {
    let mut dr_pool = DataRequestPool::default();
    let block_hash = Hash::default();
    let commit_beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let secret_key = SecretKey { bytes: [0xcd; 32] };

    let dro = DataRequestOutput {
        value: 1000,
        witnesses: 1,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    let dr_epoch = 0;
    dr_pool.process_data_request(&dr_transaction, dr_epoch);
    dr_pool.update_data_request_stages();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof =
        DataRequestEligibilityClaim::create(vrf, &secret_key, commit_beacon, dr_hash).unwrap();
    // Sign commitment
    let cs = sign_t(&cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    dr_pool.process_commit(&c_tx, &block_hash).unwrap();
    dr_pool.update_data_request_stages();

    let x = validate_commit_transaction(&c_tx, &dr_pool, commit_beacon, vrf, &rep_eng);
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NotCommitStage,
    );
}

#[test]
fn commitment_valid() {
    let x = test_commit();
    assert_eq!(x.unwrap(), ());
}

fn dr_pool_with_dr_in_reveal_stage() -> (DataRequestPool, Hash) {
    let mut dr_pool = DataRequestPool::default();
    let block_hash = Hash::default();
    let epoch = 0;
    let dro = DataRequestOutput {
        value: 1000,
        witnesses: 1,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_pointer = dr_transaction.hash();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_pointer;
    // Sign commitment
    let cs = sign_t(&cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    dr_pool.process_data_request(&dr_transaction, epoch);
    dr_pool.update_data_request_stages();
    dr_pool.process_commit(&c_tx, &block_hash).unwrap();
    dr_pool.update_data_request_stages();

    (dr_pool, dr_pointer)
}

#[test]
fn reveal_signatures() {
    let (dr_pool, dr_pointer) = dr_pool_with_dr_in_reveal_stage();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    rb.pkh = MY_PKH.parse().unwrap();

    let f = |rb, rs| {
        let r_tx = RevealTransaction::new(rb, vec![rs]);

        validate_reveal_transaction(&r_tx, &dr_pool)
    };

    let hashable = rb;

    let ks = sign_t(&hashable);
    let hash = hashable.hash();

    // Replace the signature with default (all zeros)
    let ks_default = KeyedSignature::default();
    let x = f(hashable.clone(), ks_default);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: Secp256k1ConversionError::FailSignatureConversion.to_string(),
        },
    );

    // Replace the signature with an empty vector
    let mut ks_empty = ks.clone();
    match ks_empty.signature {
        Signature::Secp256k1(ref mut x) => x.der = vec![],
    }
    let x = f(hashable.clone(), ks_empty);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: Secp256k1ConversionError::FailSignatureConversion.to_string(),
        },
    );

    // Flip one bit in the signature
    let mut ks_wrong = ks.clone();
    match ks_wrong.signature {
        Signature::Secp256k1(ref mut x) => x.der[10] ^= 0x1,
    }
    let x = f(hashable.clone(), ks_wrong);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: "Fail in verify process".to_string(),
        },
    );

    // Flip one bit in the public key of the signature
    let mut ks_bad_pk = ks.clone();
    ks_bad_pk.public_key.bytes[13] ^= 0x01;
    let x = f(hashable.clone(), ks_bad_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            index: 0,
            msg: "Fail in verify process".to_string(),
        }
    );

    // Sign transaction with a different public key
    let ks_different_pk = sign_t2(&hashable);
    let signature_pkh = ks_different_pk.public_key.pkh();
    let x = f(hashable.clone(), ks_different_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::PublicKeyHashMismatch {
            expected_pkh: MY_PKH.parse().unwrap(),
            signature_pkh,
        }
    );
}

#[test]
fn reveal_dr_in_commit_stage() {
    let mut dr_pool = DataRequestPool::default();
    let epoch = 0;
    let dro = DataRequestOutput {
        value: 1000,
        witnesses: 1,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_pointer = dr_transaction.hash();
    dr_pool.add_data_request(epoch, dr_transaction);

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    let rs = sign_t(&rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NotRevealStage,
    );
}

#[test]
fn reveal_no_signature() {
    let (dr_pool, dr_pointer) = dr_pool_with_dr_in_reveal_stage();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    let r_tx = RevealTransaction::new(rb, vec![]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::SignatureNotFound,
    );
}

#[test]
fn reveal_wrong_signature_public_key() {
    let (dr_pool, dr_pointer) = dr_pool_with_dr_in_reveal_stage();

    let bad_pkh = PublicKeyHash::default();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    rb.pkh = bad_pkh;
    let rs = sign_t(&rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::PublicKeyHashMismatch {
            expected_pkh: bad_pkh,
            signature_pkh: MY_PKH.parse().unwrap(),
        }
    );
}

#[test]
fn reveal_unknown_dr() {
    let dr_pool = DataRequestPool::default();
    let dr_pointer = "2222222222222222222222222222222222222222222222222222222222222222"
        .parse()
        .unwrap();
    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    let rs = sign_t(&rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DataRequestNotFound { hash: dr_pointer },
    );
}

#[test]
fn reveal_no_commitment() {
    let (dr_pool, dr_pointer) = dr_pool_with_dr_in_reveal_stage();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    rb.pkh = MY_PKH_2.parse().unwrap();
    let rs = sign_t2(&rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::CommitNotFound,
    );
}

#[test]
fn reveal_invalid_commitment() {
    let (dr_pool, dr_pointer) = dr_pool_with_dr_in_reveal_stage();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    rb.pkh = MY_PKH.parse().unwrap();
    let rs = sign_t(&rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchedCommitment,
    );
}

#[test]
fn reveal_valid_commitment() {
    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 5,
        reveal_fee: 100,
        ..DataRequestOutput::default()
    };
    let dr_transaction = DRTransaction {
        body: DRTransactionBody::new(vec![], vec![], dr_output),
        ..DRTransaction::default()
    };
    let dr_pointer = dr_transaction.hash();

    // Include DRTransaction in DataRequestPool
    dr_pool.process_data_request(&dr_transaction, epoch);
    dr_pool.update_data_request_stages();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key.clone();

    // Create Reveal and Commit
    let reveal_body = RevealTransactionBody::new(dr_pointer, vec![], public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::new(
            dr_pointer,
            commitment,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key: public_key.clone(),
        }],
    );
    let reveal_transaction = RevealTransaction::new(reveal_body, vec![reveal_signature]);

    // Include CommitTransaction in DataRequestPool
    dr_pool
        .process_commit(&commit_transaction, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    let (h, n, fee) = validate_reveal_transaction(&reveal_transaction, &dr_pool).unwrap();
    assert_eq!(h, dr_pointer);
    assert_eq!(n, 5);
    assert_eq!(fee, 100);

    // Create other reveal
    let reveal_body2 = RevealTransactionBody::new(
        dr_pointer,
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        public_key.pkh(),
    );
    let reveal_signature2 = sign_t(&reveal_body2);
    let reveal_transaction2 = RevealTransaction::new(reveal_body2, vec![reveal_signature2]);

    let error = validate_reveal_transaction(&reveal_transaction2, &dr_pool);
    assert_eq!(
        error.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchedCommitment
    );
}

fn dr_pool_with_dr_in_tally_stage(reveal_value: Vec<u8>) -> (DataRequestPool, Hash, PublicKeyHash) {
    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 5,
        reveal_fee: 100,
        value: 1100,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };
    let dr_transaction = DRTransaction {
        body: DRTransactionBody::new(vec![], vec![], dr_output),
        ..DRTransaction::default()
    };
    let dr_pointer = dr_transaction.hash();

    // Include DRTransaction in DataRequestPool
    dr_pool.process_data_request(&dr_transaction, epoch);
    dr_pool.update_data_request_stages();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key.clone();

    // Create Reveal and Commit
    // Reveal = empty array
    let reveal_body = RevealTransactionBody::new(dr_pointer, reveal_value, public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::new(
            dr_pointer,
            commitment,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key: public_key.clone(),
        }],
    );
    let reveal_transaction = RevealTransaction::new(reveal_body, vec![reveal_signature]);

    // Include CommitTransaction in DataRequestPool
    dr_pool
        .process_commit(&commit_transaction, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    dr_pool
        .process_reveal(&reveal_transaction, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    (dr_pool, dr_pointer, public_key.pkh())
}

fn dr_pool_with_dr_in_tally_stage_2_reveals(
    reveal_value: Vec<u8>,
) -> (DataRequestPool, Hash, PublicKeyHash, PublicKeyHash) {
    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 2,
        reveal_fee: 100,
        value: 1100,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };
    let dr_transaction = DRTransaction {
        body: DRTransactionBody::new(vec![], vec![], dr_output),
        ..DRTransaction::default()
    };
    let dr_pointer = dr_transaction.hash();

    // Include DRTransaction in DataRequestPool
    dr_pool.process_data_request(&dr_transaction, epoch);
    dr_pool.update_data_request_stages();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key.clone();
    let public_key2 = sign_t2(&RevealTransactionBody::default())
        .public_key
        .clone();

    // Create Reveal and Commit
    // Reveal = empty array
    let reveal_body =
        RevealTransactionBody::new(dr_pointer, reveal_value.clone(), public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::new(
            dr_pointer,
            commitment,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key: public_key.clone(),
        }],
    );
    let reveal_transaction = RevealTransaction::new(reveal_body, vec![reveal_signature]);

    let reveal_body2 = RevealTransactionBody::new(dr_pointer, reveal_value, public_key2.pkh());
    let reveal_signature2 = sign_t2(&reveal_body2);
    let commitment2 = reveal_signature2.signature.hash();

    let commit_transaction2 = CommitTransaction::new(
        CommitTransactionBody::new(
            dr_pointer,
            commitment2,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key: public_key2.clone(),
        }],
    );
    let reveal_transaction2 = RevealTransaction::new(reveal_body2, vec![reveal_signature2]);

    // Include CommitTransaction in DataRequestPool
    dr_pool
        .process_commit(&commit_transaction, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction2, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    dr_pool
        .process_reveal(&reveal_transaction, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_reveal(&reveal_transaction2, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    (dr_pool, dr_pointer, public_key.pkh(), public_key2.pkh())
}

#[test]
fn tally_dr_not_tally_stage() {
    // Check that data request exists and is in tally stage

    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 5,
        reveal_fee: 100,
        value: 1100,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };
    let dr_transaction = DRTransaction {
        body: DRTransactionBody::new(vec![], vec![], dr_output),
        ..DRTransaction::default()
    };
    let dr_pointer = dr_transaction.hash();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key.clone();

    // Create Reveal and Commit
    // Reveal = integer(0)
    let reveal_value = vec![0x00];
    let reveal_body = RevealTransactionBody::new(dr_pointer, reveal_value, public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::new(
            dr_pointer,
            commitment,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key: public_key.clone(),
        }],
    );
    let reveal_transaction = RevealTransaction::new(reveal_body, vec![reveal_signature]);
    // Tally value: [integer(0)]
    let tally_value = vec![0x91, 0x00];
    let vt0 = ValueTransferOutput {
        pkh: public_key.pkh(),
        value: 200,
    };
    let vt_change = ValueTransferOutput {
        pkh: PublicKeyHash::default(),
        value: 800,
    };
    let tally_transaction =
        TallyTransaction::new(dr_pointer, tally_value.clone(), vec![vt0, vt_change]);

    let mut dr_pool = DataRequestPool::default();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DataRequestNotFound { hash: dr_pointer },
    );
    dr_pool.process_data_request(&dr_transaction, epoch);
    dr_pool.update_data_request_stages();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NotTallyStage
    );

    dr_pool
        .process_commit(&commit_transaction, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NotTallyStage
    );

    dr_pool
        .process_reveal(&reveal_transaction, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(x.unwrap(), (),);
}

#[test]
fn tally_invalid_consensus() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh) = dr_pool_with_dr_in_tally_stage(reveal_value);

    // Tally value: [integer(0)]
    let tally_value = vec![0x91, 0x00];
    // Fake tally value: [integer(1)]
    let fake_tally_value = vec![0x01];

    let vt0 = ValueTransferOutput { pkh, value: 200 };
    let vt_change = ValueTransferOutput {
        pkh: PublicKeyHash::default(),
        value: 800,
    };

    let tally_transaction =
        TallyTransaction::new(dr_pointer, fake_tally_value.clone(), vec![vt0, vt_change]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchedConsensus {
            local_tally: tally_value,
            miner_tally: fake_tally_value,
        }
    );
}

#[test]
fn tally_valid() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh) = dr_pool_with_dr_in_tally_stage(reveal_value);

    // Tally value: [integer(0)]
    let tally_value = vec![0x91, 0x00];
    let vt0 = ValueTransferOutput { pkh, value: 200 };
    let vt_change = ValueTransferOutput {
        pkh: PublicKeyHash::default(),
        value: 800,
    };
    let tally_transaction =
        TallyTransaction::new(dr_pointer, tally_value.clone(), vec![vt0, vt_change]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(x.unwrap(), ());
}

#[test]
fn tally_too_many_outputs() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh) = dr_pool_with_dr_in_tally_stage(reveal_value);

    // Tally value: [integer(0)]
    let tally_value = vec![0x91, 0x00];
    let vt0 = ValueTransferOutput { pkh, value: 200 };
    let vt1 = ValueTransferOutput { pkh, value: 200 };
    let vt2 = ValueTransferOutput { pkh, value: 200 };
    let vt3 = ValueTransferOutput { pkh, value: 200 };
    let vt_change = ValueTransferOutput {
        pkh: PublicKeyHash::default(),
        value: 800,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value.clone(),
        vec![vt0, vt1, vt2, vt3, vt_change],
    );
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::WrongNumberOutputs {
            outputs: tally_transaction.outputs.len(),
            expected_outputs: 2
        },
    );
}

#[test]
fn tally_too_less_outputs() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh, _pkh2) = dr_pool_with_dr_in_tally_stage_2_reveals(reveal_value);

    // Tally value: [integer(0), integer(0)]
    let tally_value = vec![0x92, 0x00, 0x00];
    let vt0 = ValueTransferOutput { pkh, value: 500 };

    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value.clone(), vec![vt0]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::WrongNumberOutputs {
            outputs: tally_transaction.outputs.len(),
            expected_outputs: 2
        },
    );
}

#[test]
fn tally_invalid_change() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh) = dr_pool_with_dr_in_tally_stage(reveal_value);

    // Tally value: [integer(0)]
    let tally_value = vec![0x91, 0x00];
    let vt0 = ValueTransferOutput { pkh, value: 200 };
    let vt_change = ValueTransferOutput {
        pkh: PublicKeyHash::default(),
        value: 1000,
    };
    let tally_transaction =
        TallyTransaction::new(dr_pointer, tally_value.clone(), vec![vt0, vt_change]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidTallyChange {
            change: 1000,
            expected_change: 800
        },
    );
}

#[test]
fn tally_double_reward() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh, _pkh2) = dr_pool_with_dr_in_tally_stage_2_reveals(reveal_value);

    // Tally value: [integer(0), integer(0)]
    let tally_value = vec![0x92, 0x00, 0x00];
    let vt0 = ValueTransferOutput { pkh, value: 500 };
    let vt1 = ValueTransferOutput { pkh, value: 500 };
    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value.clone(), vec![vt0, vt1]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MultipleRewards { pkh },
    );
}

// TODO: Create a test to check the true_revealer function that will be implemented in FIXME(#640)

#[test]
fn tally_reveal_not_found() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh, _pkh2) = dr_pool_with_dr_in_tally_stage_2_reveals(reveal_value);

    // Tally value: [integer(0), integer(0)]
    let tally_value = vec![0x92, 0x00, 0x00];
    let vt0 = ValueTransferOutput { pkh, value: 500 };
    let vt1 = ValueTransferOutput {
        pkh: PublicKeyHash::default(),
        value: 500,
    };
    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value.clone(), vec![vt0, vt1]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::RevealNotFound,
    );
}

#[test]
fn tally_valid_2_reveals() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh, pkh2) = dr_pool_with_dr_in_tally_stage_2_reveals(reveal_value);

    // Tally value: [integer(0), integer(0)]
    let tally_value = vec![0x92, 0x00, 0x00];
    let vt0 = ValueTransferOutput { pkh, value: 500 };
    let vt1 = ValueTransferOutput {
        pkh: pkh2,
        value: 500,
    };
    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value.clone(), vec![vt0, vt1]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(x.unwrap(), ());
}

#[test]
fn block_signatures() {
    let mut b = Block {
        block_header: Default::default(),
        block_sig: Default::default(),
        txns: Default::default(),
    };
    // Add valid vrf proof
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let secret_key = SecretKey { bytes: [0xcd; 32] };
    b.block_header.proof =
        BlockEligibilityClaim::create(vrf, &secret_key, b.block_header.beacon).unwrap();

    let hashable = b;
    let f = |mut b: Block, ks| {
        b.block_sig = ks;
        validate_block_signature(&b)
    };

    let ks = sign_t(&hashable);
    let hash = hashable.hash();

    // Replace the signature with default (all zeros)
    let ks_default = KeyedSignature::default();
    let signature_pkh = ks_default.public_key.pkh();
    let x = f(hashable.clone(), ks_default);
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::PublicKeyHashMismatch {
            proof_pkh: MY_PKH.parse().unwrap(),
            signature_pkh,
        }
    );

    // Replace the signature with an empty vector
    let mut ks_empty = ks.clone();
    match ks_empty.signature {
        Signature::Secp256k1(ref mut x) => x.der = vec![],
    }
    let x = f(hashable.clone(), ks_empty);
    assert_eq!(
        x.unwrap_err()
            .downcast::<Secp256k1ConversionError>()
            .unwrap(),
        Secp256k1ConversionError::FailSignatureConversion
    );

    // Flip one bit in the signature
    let mut ks_wrong = ks.clone();
    match ks_wrong.signature {
        Signature::Secp256k1(ref mut x) => x.der[10] ^= 0x1,
    }
    let x = f(hashable.clone(), ks_wrong);
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::VerifySignatureFail { hash }
    );

    // Flip one bit in the public key of the signature
    let mut ks_bad_pk = ks.clone();
    ks_bad_pk.public_key.bytes[13] ^= 0x01;
    let signature_pkh = ks_bad_pk.public_key.pkh();
    let x = f(hashable.clone(), ks_bad_pk);
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::PublicKeyHashMismatch {
            proof_pkh: MY_PKH.parse().unwrap(),
            signature_pkh,
        }
    );

    // Sign transaction with a different public key
    let ks_different_pk = sign_t2(&hashable);
    let signature_pkh = ks_different_pk.public_key.pkh();
    let x = f(hashable.clone(), ks_different_pk);
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::PublicKeyHashMismatch {
            proof_pkh: MY_PKH.parse().unwrap(),
            signature_pkh,
        }
    );
}

static MILLION_TX_OUTPUT: &str =
    "0f0f000000000000000000000000000000000000000000000000000000000000:0";

fn test_block<F: FnMut(&mut Block) -> bool>(mut mut_block: F) -> Result<(), failure::Error> {
    let dr_pool = DataRequestPool::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let mut utxo_set = UnspentOutputsPool::default();
    // Insert output to utxo
    let output1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000_000,
    };
    //let tx_output1 = VTTransactionBody::new(vec![], vec![output1.clone()]);
    //let output1_pointer = OutputPointer { transaction_id: tx_output1.hash(), output_index: 0 };
    let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
    utxo_set.insert(output1_pointer, output1);

    let secret_key = SecretKey { bytes: [0xcd; 32] };
    let current_epoch = 1000;
    let genesis_block_hash = Hash::default();
    let last_block_hash = "62adde3e36db3f22774cc255215b2833575f66bf2204011f80c03d34c7c9ea41"
        .parse()
        .unwrap();
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let block_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let my_pkh = PublicKeyHash::default();

    let mut txns = BlockTransactions::default();
    txns.mint = MintTransaction::new(
        current_epoch,
        ValueTransferOutput {
            pkh: my_pkh,
            value: block_reward(current_epoch),
        },
    );

    let mut block_header = BlockHeader::default();
    build_merkle_tree(&mut block_header, &txns);
    block_header.beacon = block_beacon;
    block_header.proof = BlockEligibilityClaim::create(vrf, &secret_key, block_beacon).unwrap();

    let block_sig = sign_t(&block_header);
    let mut b = Block {
        block_header,
        block_sig,
        txns,
    };

    // Pass the block to the mutation function used by tests
    if mut_block(&mut b) {
        // If the function returns true, re-sign the block after mutating
        b.block_sig = sign_t(&b.block_header);
    }

    validate_candidate(
        &b,
        current_epoch,
        vrf,
        rep_eng.ars.active_identities_number() as u32,
    )?;

    validate_block(
        &b,
        current_epoch,
        chain_beacon,
        genesis_block_hash,
        &utxo_set,
        &dr_pool,
        vrf,
        &rep_eng,
    )
    .map(|_| ())
}

fn build_merkle_tree(block_header: &mut BlockHeader, txns: &BlockTransactions) {
    let merkle_roots = BlockMerkleRoots {
        mint_hash: txns.mint.hash(),
        vt_hash_merkle_root: merkle_tree_root(&txns.value_transfer_txns),
        dr_hash_merkle_root: merkle_tree_root(&txns.data_request_txns),
        commit_hash_merkle_root: merkle_tree_root(&txns.commit_txns),
        reveal_hash_merkle_root: merkle_tree_root(&txns.reveal_txns),
        tally_hash_merkle_root: merkle_tree_root(&txns.tally_txns),
    };
    block_header.merkle_roots = merkle_roots;
}

///////////////////////////////////////////////////////////////////////////////
// Block tests: one block
///////////////////////////////////////////////////////////////////////////////

#[test]
fn block_from_the_future() {
    let current_epoch = 1000;
    let block_epoch = current_epoch + 1;

    let x = test_block(|b| {
        assert_eq!(current_epoch, b.block_header.beacon.checkpoint);
        b.block_header.beacon.checkpoint = block_epoch;

        true
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::CandidateFromDifferentEpoch {
            current_epoch,
            block_epoch
        }
    );
}

#[test]
fn block_from_the_past() {
    let current_epoch = 1000;
    let block_epoch = current_epoch - 1;

    let x = test_block(|b| {
        assert_eq!(current_epoch, b.block_header.beacon.checkpoint);
        b.block_header.beacon.checkpoint = block_epoch;

        true
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::CandidateFromDifferentEpoch {
            current_epoch,
            block_epoch
        },
    );
}

#[test]
fn block_unknown_hash_prev_block() {
    let unknown_hash = "2222222222222222222222222222222222222222222222222222222222222222"
        .parse()
        .unwrap();

    let x = test_block(|b| {
        assert_ne!(unknown_hash, b.block_header.beacon.hash_prev_block);
        b.block_header.beacon.hash_prev_block = unknown_hash;

        // Re-create a valid VRF proof
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey { bytes: [0xcd; 32] };

        b.block_header.proof =
            BlockEligibilityClaim::create(vrf, &secret_key, b.block_header.beacon).unwrap();

        true
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::PreviousHashNotKnown { hash: unknown_hash },
    );
}

#[test]
fn block_invalid_poe() {
    let x = test_block(|b| {
        b.block_header.proof = BlockEligibilityClaim::default();

        true
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::NotValidPoe,
    );
}

#[test]
fn block_difficult_proof() {
    let dr_pool = DataRequestPool::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();

    // Create a reputation engine with 512 identities
    let mut rep_eng = ReputationEngine::new(100);
    rep_eng
        .ars
        .push_activity((0..512).map(|x| format!("{:040}", x).parse().unwrap()));
    let mut utxo_set = UnspentOutputsPool::default();
    // Insert output to utxo
    let output1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000_000,
    };
    //let tx_output1 = VTTransactionBody::new(vec![], vec![output1.clone()]);
    //let output1_pointer = OutputPointer { transaction_id: tx_output1.hash(), output_index: 0 };
    let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
    utxo_set.insert(output1_pointer, output1);

    let secret_key = SecretKey { bytes: [0xcd; 32] };
    let current_epoch = 1000;
    let genesis_block_hash = Hash::default();
    let last_block_hash = "62adde3e36db3f22774cc255215b2833575f66bf2204011f80c03d34c7c9ea41"
        .parse()
        .unwrap();
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let block_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let my_pkh = PublicKeyHash::default();

    let mut txns = BlockTransactions::default();
    txns.mint = MintTransaction::new(
        current_epoch,
        ValueTransferOutput {
            pkh: my_pkh,
            value: block_reward(current_epoch),
        },
    );

    let mut block_header = BlockHeader::default();
    build_merkle_tree(&mut block_header, &txns);
    block_header.beacon = block_beacon;
    block_header.proof = BlockEligibilityClaim::create(vrf, &secret_key, block_beacon).unwrap();

    let block_sig = sign_t(&block_header);
    let b = Block {
        block_header,
        block_sig,
        txns,
    };

    let x = {
        let mut x = || {
            validate_candidate(
                &b,
                current_epoch,
                vrf,
                rep_eng.ars.active_identities_number() as u32,
            )?;

            validate_block(
                &b,
                current_epoch,
                chain_beacon,
                genesis_block_hash,
                &utxo_set,
                &dr_pool,
                vrf,
                &rep_eng,
            )
            .map(|_| ())
        };

        x()
    };

    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::BlockEligibilityDoesNotMeetTarget {
            vrf_hash: "40167423312aad76b13613d822d8fc677b8db84667202c33fbbaeb3008906bdc"
                .parse()
                .unwrap(),
            target_hash: Hash::with_first_u32(0x007f_ffff),
        },
    );
}

#[test]
fn block_change_mint() {
    let x = test_block(|b| {
        assert_ne!(b.txns.mint.output.pkh, MY_PKH.parse().unwrap());
        b.txns.mint = MintTransaction::new(
            b.txns.mint.epoch,
            ValueTransferOutput {
                pkh: MY_PKH.parse().unwrap(),
                ..b.txns.mint.output
            },
        );

        true
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::NotValidMerkleTree,
    );
}

#[test]
fn block_add_vtt_but_dont_update_mint() {
    let mut old_mint_value = None;
    let x = test_block(|b| {
        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 1,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0]);
        let vts = sign_t(&vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);
        b.txns.value_transfer_txns.push(vt_tx);

        old_mint_value = Some(b.txns.mint.output.value);

        true
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::MismatchedMintValue {
            mint_value: old_mint_value.unwrap(),
            fees_value: 1_000_000 - 1,
            reward_value: old_mint_value.unwrap(),
        },
    );
}

#[test]
fn block_add_vtt_but_dont_update_merkle_tree() {
    let x = test_block(|b| {
        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 1,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0]);
        let vts = sign_t(&vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);
        b.txns.value_transfer_txns.push(vt_tx);

        b.txns.mint = MintTransaction::new(
            b.txns.mint.epoch,
            ValueTransferOutput {
                value: b.txns.mint.output.value + 1_000_000 - 1,
                ..b.txns.mint.output
            },
        );

        true
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::NotValidMerkleTree,
    );
}

///////////////////////////////////////////////////////////////////////////////
// Malleability tests: can we change a block without invalidating it?
///////////////////////////////////////////////////////////////////////////////

#[test]
fn block_change_signature() {
    // Signing a block with a different public key invalidates the BlockEligibilityClaim
    let mut old_pkh = None;
    let mut new_pkh = None;
    let x = test_block(|b| {
        old_pkh = Some(b.block_sig.public_key.pkh());
        // Sign with a different key
        b.block_sig = sign_t2(&b.block_header);
        new_pkh = Some(b.block_sig.public_key.pkh());
        assert_ne!(old_pkh, new_pkh);

        false
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::PublicKeyHashMismatch {
            proof_pkh: old_pkh.unwrap(),
            signature_pkh: new_pkh.unwrap(),
        },
    );
}

#[test]
fn block_change_hash_prev_block() {
    let x = test_block(|b| {
        let fake_hash = Hash::default();
        b.block_header.beacon.hash_prev_block = fake_hash;

        false
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::NotValidPoe,
    );
}

#[test]
fn block_change_merkle_tree() {
    let x = test_block(|b| {
        let unknown_hash = "2222222222222222222222222222222222222222222222222222222222222222"
            .parse()
            .unwrap();
        b.block_header.merkle_roots.reveal_hash_merkle_root = unknown_hash;

        false
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::VerifySignatureFail {
            hash: "4eebf363d7e67ea3d4214581e2f39b62bdfee350eb0e99870f632213f490848e"
                .parse()
                .unwrap()
        },
    );
}

///////////////////////////////////////////////////////////////////////////////
// Block transaction tests: multiple blocks in sequence
///////////////////////////////////////////////////////////////////////////////

fn test_blocks(txns: Vec<(BlockTransactions, u64)>) -> Result<(), failure::Error> {
    if txns.len() > 1 {
        // FIXME(#685): add sequence validations
        unimplemented!();
    }

    let dr_pool = DataRequestPool::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let mut utxo_set = UnspentOutputsPool::default();
    // Insert output to utxo
    let output1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000_000,
    };
    //let tx_output1 = VTTransactionBody::new(vec![], vec![output1.clone()]);
    //let output1_pointer = OutputPointer { transaction_id: tx_output1.hash(), output_index: 0 };
    let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
    utxo_set.insert(output1_pointer, output1);

    let secret_key = SecretKey { bytes: [0xcd; 32] };
    let mut current_epoch = 1000;
    let genesis_block_hash = Hash::default();
    let mut last_block_hash = "62adde3e36db3f22774cc255215b2833575f66bf2204011f80c03d34c7c9ea41"
        .parse()
        .unwrap();
    let my_pkh = PublicKeyHash::default();

    for (mut txns, fees) in txns {
        // Rebuild mint
        txns.mint = MintTransaction::new(
            current_epoch,
            ValueTransferOutput {
                pkh: my_pkh,
                value: block_reward(current_epoch) + fees,
            },
        );

        let chain_beacon = CheckpointBeacon {
            checkpoint: current_epoch,
            hash_prev_block: last_block_hash,
        };
        let block_beacon = CheckpointBeacon {
            checkpoint: current_epoch,
            hash_prev_block: last_block_hash,
        };
        let mut block_header = BlockHeader::default();
        build_merkle_tree(&mut block_header, &txns);
        block_header.beacon = block_beacon;
        block_header.proof = BlockEligibilityClaim::create(vrf, &secret_key, block_beacon).unwrap();

        let block_sig = KeyedSignature::default();
        let mut b = Block {
            block_header,
            block_sig,
            txns,
        };

        b.block_sig = sign_t(&b.block_header);

        // First, validate candidate block (can return false positives)
        validate_candidate(
            &b,
            current_epoch,
            vrf,
            rep_eng.ars.active_identities_number() as u32,
        )?;

        // Do the expensive validation
        validate_block(
            &b,
            current_epoch,
            chain_beacon,
            genesis_block_hash,
            &utxo_set,
            &dr_pool,
            vrf,
            &rep_eng,
        )?;

        // FIXME(#685): add sequence validations
        //update_pools(&b)?;

        current_epoch += 1;
        last_block_hash = b.hash();
    }

    Ok(())
}

#[test]
fn block_minimum_valid() {
    let t0 = {
        // total fee excluding mint:
        let extra_fee = 0;

        (
            BlockTransactions {
                // No need to set the mint as it is overwritten in test_blocks
                mint: Default::default(),
                ..BlockTransactions::default()
            },
            extra_fee,
        )
    };
    let x = test_blocks(vec![t0]);
    assert_eq!(x.unwrap(), (),);
}

#[test]
fn block_add_vtt_no_inputs() {
    let vt_tx_hash;
    let t0 = {
        // (actually the fee is -1)
        let extra_fee = 0;
        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 1,
        };
        let vt_body = VTTransactionBody::new(vec![], vec![vto0]);
        let vt_tx = VTTransaction::new(vt_body, vec![]);
        vt_tx_hash = vt_tx.hash();

        (
            BlockTransactions {
                value_transfer_txns: vec![vt_tx],
                ..BlockTransactions::default()
            },
            extra_fee,
        )
    };

    let x = test_blocks(vec![t0]);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NoInputs {
            tx_hash: vt_tx_hash,
        }
    );
}

#[test]
fn block_add_vtt() {
    let t0 = {
        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0]);
        let vts = sign_t(&vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);

        (
            BlockTransactions {
                value_transfer_txns: vec![vt_tx],
                ..BlockTransactions::default()
            },
            1_000_000 - 10,
        )
    };
    let x = test_blocks(vec![t0]);
    assert_eq!(x.unwrap(), (),);
}

#[test]
fn block_add_2_vtt_same_input() {
    let t0 = {
        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 1,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0]);
        let vts = sign_t(&vt_body);
        let vt_tx1 = VTTransaction::new(vt_body, vec![vts]);

        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 1,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0]);
        let vts = sign_t(&vt_body);
        let vt_tx2 = VTTransaction::new(vt_body, vec![vts]);

        (
            BlockTransactions {
                value_transfer_txns: vec![vt_tx1, vt_tx2],
                ..BlockTransactions::default()
            },
            (1_000_000 - 10) * 2,
        )
    };

    let x = test_blocks(vec![t0]);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::OutputNotFound {
            output: MILLION_TX_OUTPUT.parse().unwrap(),
        },
    );
}

// FIXME(#685): add sequence validations
#[ignore]
#[test]
fn block_vtt_sequence() {
    let t0_hash;
    let t0 = {
        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 1_000_000 - 10,
        };
        let output0_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output0_pointer)], vec![vto0]);
        t0_hash = vt_body.hash();
        let vts = sign_t(&vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);

        (
            BlockTransactions {
                value_transfer_txns: vec![vt_tx],
                ..BlockTransactions::default()
            },
            10,
        )
    };

    let t1 = {
        let o1 = ValueTransferOutput {
            pkh: Default::default(),
            value: 1_000_000 - 10 - 20,
        };
        let output1_pointer = OutputPointer {
            transaction_id: t0_hash,
            output_index: 0,
        };
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![o1]);
        let vts = sign_t(&vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);

        (
            BlockTransactions {
                value_transfer_txns: vec![vt_tx],
                ..BlockTransactions::default()
            },
            20,
        )
    };

    let x = test_blocks(vec![t0, t1]);
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::VerifySignatureFail {
            hash: "4eebf363d7e67ea3d4214581e2f39b62bdfee350eb0e99870f632213f490848e"
                .parse()
                .unwrap()
        },
    );
}

#[test]
fn block_add_drt() {
    let t0 = {
        let data_request = example_data_request();
        let dr_output = DataRequestOutput {
            value: 750,
            witnesses: 2,
            data_request,
            ..DataRequestOutput::default()
        };

        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let dr_tx_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0], dr_output);
        let drs = sign_t(&dr_tx_body);
        let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

        (
            BlockTransactions {
                data_request_txns: vec![dr_transaction],
                ..BlockTransactions::default()
            },
            1_000_000 - 750 - 10,
        )
    };
    let x = test_blocks(vec![t0]);
    assert_eq!(x.unwrap(), (),);
}

#[test]
fn block_add_2_drt_same_input() {
    let t0 = {
        let data_request = example_data_request();
        let dr_output = DataRequestOutput {
            value: 750,
            witnesses: 2,
            data_request,
            ..DataRequestOutput::default()
        };

        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let dr_tx_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0], dr_output);
        let drs = sign_t(&dr_tx_body);
        let dr_tx1 = DRTransaction::new(dr_tx_body, vec![drs]);

        let data_request = example_data_request();
        let dr_output = DataRequestOutput {
            value: 750,
            witnesses: 2,
            data_request,
            ..DataRequestOutput::default()
        };

        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let dr_tx_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0], dr_output);
        let drs = sign_t(&dr_tx_body);
        let dr_tx2 = DRTransaction::new(dr_tx_body, vec![drs]);

        (
            BlockTransactions {
                data_request_txns: vec![dr_tx1, dr_tx2],
                ..BlockTransactions::default()
            },
            1_000_000 - 750 - 10,
        )
    };
    let x = test_blocks(vec![t0]);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::OutputNotFound {
            output: MILLION_TX_OUTPUT.parse().unwrap(),
        },
    );
}

#[test]
fn block_add_1_drt_and_1_vtt_same_input() {
    let t0 = {
        let data_request = example_data_request();
        let dr_output = DataRequestOutput {
            value: 750,
            witnesses: 2,
            data_request,
            ..DataRequestOutput::default()
        };

        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let dr_tx_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0], dr_output);
        let drs = sign_t(&dr_tx_body);
        let dr_tx = DRTransaction::new(dr_tx_body, vec![drs]);

        let vto0 = ValueTransferOutput {
            pkh: Default::default(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0]);
        let vts = sign_t(&vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);

        (
            BlockTransactions {
                data_request_txns: vec![dr_tx],
                value_transfer_txns: vec![vt_tx],
                ..BlockTransactions::default()
            },
            1_000_000 - 750 - 10 - 10,
        )
    };
    let x = test_blocks(vec![t0]);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::OutputNotFound {
            output: MILLION_TX_OUTPUT.parse().unwrap(),
        },
    );
}
