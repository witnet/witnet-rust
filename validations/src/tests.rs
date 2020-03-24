use itertools::Itertools;
use std::{
    collections::HashSet,
    convert::TryFrom,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::validations::*;
use witnet_crypto::{
    key::CryptoEngine,
    secp256k1::{PublicKey as Secp256k1_PublicKey, Secp256k1, SecretKey as Secp256k1_SecretKey},
    signature::sign,
};
use witnet_data_structures::{
    chain::*,
    data_request::{create_tally, DataRequestPool},
    error::{BlockError, DataRequestError, Secp256k1ConversionError, TransactionError},
    radon_report::{RadonReport, ReportContext, TypeLike},
    transaction::*,
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim, VrfCtx},
};
use witnet_protected::Protected;
use witnet_rad::{
    error::RadError,
    filters::RadonFilters,
    reducers::RadonReducers,
    types::{integer::RadonInteger, RadonTypes},
};

static MY_PKH: &str = "wit18cfejmk3305y9kw5xqa59rwnpjzahr57us48vm";
static MY_PKH_2: &str = "wit1z8mxkml4a50dyysqczsp7gj5pnvz3jsldras8t";
static MY_PKH_3: &str = "wit164gu2l8p7suvc7zq5xvc27h63td75g6uspwpn5";
static ONE_WIT: u64 = 1_000_000_000;

fn verify_signatures_test(
    signatures_to_verify: Vec<SignaturesToVerify>,
) -> Result<(), failure::Error> {
    let secp = &CryptoEngine::new();
    let vrf = &mut VrfCtx::secp256k1().unwrap();

    verify_signatures(signatures_to_verify, vrf, secp).map(|_| ())
}

fn sign_t<H: Hashable>(tx: &H) -> KeyedSignature {
    let Hash::SHA256(data) = tx.hash();

    let secp = &Secp256k1::new();
    let secret_key =
        Secp256k1_SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
    let public_key = Secp256k1_PublicKey::from_secret_key(secp, &secret_key);
    let public_key = PublicKey::from(public_key);
    assert_eq!(public_key.pkh(), MY_PKH.parse().unwrap());

    let signature = sign(secp, secret_key, &data).unwrap();

    KeyedSignature {
        signature: Signature::from(signature),
        public_key,
    }
}

// Sign with a different public key
fn sign_t2<H: Hashable>(tx: &H) -> KeyedSignature {
    let Hash::SHA256(data) = tx.hash();

    let secp = &Secp256k1::new();
    let secret_key =
        Secp256k1_SecretKey::from_slice(&[0x43; 32]).expect("32 bytes, within curve order");
    let public_key = Secp256k1_PublicKey::from_secret_key(secp, &secret_key);
    let public_key = PublicKey::from(public_key);
    assert_eq!(public_key.pkh(), MY_PKH_2.parse().unwrap());

    let signature = sign(secp, secret_key, &data).unwrap();

    KeyedSignature {
        signature: Signature::from(signature),
        public_key,
    }
}

// Sign with a different public key
fn sign_t3<H: Hashable>(tx: &H) -> KeyedSignature {
    let Hash::SHA256(data) = tx.hash();

    let secp = &Secp256k1::new();
    let secret_key =
        Secp256k1_SecretKey::from_slice(&[0x69; 32]).expect("32 bytes, within curve order");
    let public_key = Secp256k1_PublicKey::from_secret_key(secp, &secret_key);
    let public_key = PublicKey::from(public_key);
    assert_eq!(public_key.pkh(), MY_PKH_3.parse().unwrap());

    let signature = sign(secp, secret_key, &data).unwrap();

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
    let block_number = 0;

    generate_unspent_outputs_pool(&all_utxos, &txns, block_number)
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
        time_lock: 0,
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
        time_lock: 0,
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
        time_lock: 0,
    };
    let mint_tx = MintTransaction::new(epoch, output);
    let x = validate_mint_transaction(&mint_tx, total_fees, epoch);
    x.unwrap();
}

#[test]
fn vtt_no_inputs_no_outputs() {
    let mut signatures_to_verify = vec![];
    let utxo_pool = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);

    let vt_body = VTTransactionBody::new(vec![], vec![]);
    let vt_tx = VTTransaction::new(vt_body, vec![]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NoInputs {
            tx_hash: vt_tx.hash(),
        }
    );
}

#[test]
fn vtt_no_inputs_zero_output() {
    let mut signatures_to_verify = vec![];
    let utxo_pool = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);

    // Try to create a data request with no inputs
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 0,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![], vec![vto0]);
    let vt_tx = VTTransaction::new(vt_body, vec![]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NoInputs {
            tx_hash: vt_tx.hash(),
        }
    );
}

#[test]
fn vtt_no_inputs() {
    let mut signatures_to_verify = vec![];
    let utxo_pool = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);

    // Try to create a data request with no inputs
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![], vec![vto0]);
    let vt_tx = VTTransaction::new(vt_body, vec![]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NoInputs {
            tx_hash: vt_tx.hash(),
        }
    );
}

#[test]
fn vtt_no_inputs_but_one_signature() {
    let mut signatures_to_verify = vec![];
    let utxo_pool = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);

    // No inputs but 1 signature
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
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
    let mut signatures_to_verify = vec![];
    let utxo_pool = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(
        "2222222222222222222222222222222222222222222222222222222222222222:0"
            .parse()
            .unwrap(),
    );

    // No signatures but 1 input
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
    let vt_tx = VTTransaction::new(vt_body, vec![]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
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
            msg: "Fail in verify process".to_string(),
        },
    );

    // Flip one bit in the public key of the signature
    let mut ks_bad_pk = ks;
    ks_bad_pk.public_key.bytes[13] ^= 0x01;
    let signature_pkh = ks_bad_pk.public_key.pkh();
    let x = f(hashable.clone(), ks_bad_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
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
    let x = f(hashable, ks_different_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
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
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);

    test_signature_empty_wrong_bad(vt_body, |vt_body, vts| {
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);
        let mut signatures_to_verify = vec![];

        validate_vt_transaction(
            &vt_tx,
            &utxo_diff,
            Epoch::default(),
            EpochConstants::default(),
            &mut signatures_to_verify,
        )?;
        verify_signatures_test(signatures_to_verify)?;

        Ok(())
    });
}

#[test]
fn vtt_input_not_in_utxo() {
    let mut signatures_to_verify = vec![];
    let utxo_pool = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(
        "2222222222222222222222222222222222222222222222222222222222222222:0"
            .parse()
            .unwrap(),
    );

    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
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
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee
    );
}

#[test]
fn vtt_one_input_zero_value_output() {
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let zero_output = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 0,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![zero_output]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
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
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 2,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1,
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 2,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0, vto1]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee,
    );
}

#[test]
fn vtt_one_input_two_outputs() {
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 21,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 13,
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 7,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0, vto1]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    )
    .map(|(_, _, fee)| fee);
    assert_eq!(x.unwrap(), 21 - 13 - 7,);
}

#[test]
fn vtt_two_inputs_one_signature() {
    let mut signatures_to_verify = vec![];
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 21,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 13,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti0 = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 10,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
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
    let mut signatures_to_verify = vec![];
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 21,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 13,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti0 = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 10,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vts2 = sign_t2(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts, vts2]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash: vt_tx.hash(),
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
    let mut signatures_to_verify = vec![];
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 21,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 13,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti0 = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 10,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts.clone(), vts.clone(), vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );
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
    let mut signatures_to_verify = vec![];
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 21,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 13,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti0 = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 10,
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 20,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0, vto1]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts.clone(), vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    )
    .map(|(_, _, fee)| fee);
    assert_eq!(x.unwrap(), 21 + 13 - 10 - 20,);
}

#[test]
fn vtt_input_value_overflow() {
    let mut signatures_to_verify = vec![];
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: u64::max_value(),
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti0 = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    // The total output value should not overflow
    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: u64::max_value() - 10,
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 10,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0, vto1]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts; 2]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InputValueOverflow
    );
}

#[test]
fn vtt_output_value_overflow() {
    let mut signatures_to_verify = vec![];
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: u64::max_value() - 1_000,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti0 = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    // The total output value should overflow
    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: u64::max_value(),
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0, vto1]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts; 2]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    );

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::OutputValueOverflow
    );
}

#[test]
fn vtt_timelock() {
    // 1 epoch = 1000 seconds, for easy testing
    let epoch_constants = EpochConstants {
        checkpoint_zero_timestamp: 0,
        checkpoints_period: 1_000,
    };

    let test_vtt_epoch = |epoch, time_lock| {
        let vto = ValueTransferOutput {
            pkh: MY_PKH.parse().unwrap(),
            value: 1000,
            time_lock,
        };
        let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
        let block_number = 0;
        let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
        let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

        let pkh = PublicKeyHash::default();
        let vto0 = ValueTransferOutput {
            pkh,
            value: 1000,
            time_lock: 0,
        };

        let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
        let vts = sign_t(&vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);
        let mut signatures_to_verify = vec![];
        validate_vt_transaction(
            &vt_tx,
            &utxo_diff,
            epoch,
            epoch_constants,
            &mut signatures_to_verify,
        )?;
        verify_signatures_test(signatures_to_verify)
    };

    // (epoch, time_lock, should_be_accepted_into_block)
    let tests = vec![
        (0, 0, true),
        (0, 1, false),
        (0, 1_000_000, false),
        (999, 1_000_000, false),
        (999, 999_999, false),
        (1000, 999_999, true),
        (1000, 1_000_000, true),
        (1000, 1_000_001, false),
        (1001, 1_000_000, true),
        (1001, 1_000_001, true),
    ];

    for (epoch, time_lock, is_ok) in tests {
        let x = test_vtt_epoch(epoch, time_lock);
        assert_eq!(x.is_ok(), is_ok, "{:?}: {:?}", (epoch, time_lock, is_ok), x);
    }
}

#[test]
fn vtt_valid() {
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
    )
    .map(|(_, _, fee)| fee);
    // The fee is 1000 - 1000 = 0
    assert_eq!(x.unwrap(), 0,);
}

#[test]
fn genesis_vtt_unexpected_input() {
    // Genesis VTTs should not have any inputs
    let pkh = PublicKeyHash::default();
    let vti0 = Input::new(OutputPointer::default());
    let inputs = vec![vti0];
    let inputs_len = inputs.len();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };
    let outputs = vec![vto0];
    let vt_body = VTTransactionBody::new(inputs, outputs);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);

    let x = validate_genesis_vt_transaction(&vt_tx);

    assert_eq!(
        x.unwrap_err(),
        TransactionError::InputsInGenesis {
            inputs_n: inputs_len,
        }
    );
}

#[test]
fn genesis_vtt_unexpected_signature() {
    // Genesis VTTs should not have any signatures
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };
    let outputs = vec![vto0];
    let vt_body = VTTransactionBody::new(vec![], outputs);
    let vts = sign_t(&vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);

    let x = validate_genesis_vt_transaction(&vt_tx);

    assert_eq!(
        x.unwrap_err(),
        TransactionError::MismatchingSignaturesNumber {
            signatures_n: 1,
            inputs_n: 0,
        }
    );
}

#[test]
fn genesis_vtt_zero_value() {
    // Genesis VTT outputs cannot have value 0
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 0,
        time_lock: 0,
    };
    let outputs = vec![vto0];
    let vt_body = VTTransactionBody::new(vec![], outputs);
    let vt_tx = VTTransaction::new(vt_body, vec![]);

    let x = validate_genesis_vt_transaction(&vt_tx);

    assert_eq!(
        x.unwrap_err(),
        TransactionError::ZeroValueOutput {
            tx_hash: vt_tx.hash(),
            output_id: 0,
        }
    );
}

#[test]
fn genesis_vtt_no_outputs() {
    // Genesis VTTs must have at least one output
    let outputs = vec![];
    let vt_body = VTTransactionBody::new(vec![], outputs);
    let vt_tx = VTTransaction::new(vt_body, vec![]);

    let x = validate_genesis_vt_transaction(&vt_tx);

    assert_eq!(x.unwrap_err(), TransactionError::NoOutputsInGenesis,);
}

#[test]
fn genesis_vtt_value_overflow() {
    let pkh = PublicKeyHash::default();
    let outputs = vec![
        ValueTransferOutput {
            pkh,
            value: u64::max_value(),
            time_lock: 0,
        },
        ValueTransferOutput {
            pkh,
            value: u64::max_value(),
            time_lock: 0,
        },
    ];
    let vt_body = VTTransactionBody::new(vec![], outputs);
    let vt_tx = VTTransaction::new(vt_body, vec![]);

    let x = validate_genesis_vt_transaction(&vt_tx);

    assert_eq!(x.unwrap_err(), TransactionError::OutputValueOverflow);
}

#[test]
fn genesis_vtt_valid() {
    let pkh = PublicKeyHash::default();
    let value = 1000;
    let vto0 = ValueTransferOutput {
        pkh,
        value,
        time_lock: 0,
    };
    let outputs = vec![vto0.clone()];
    let vt_body = VTTransactionBody::new(vec![], outputs.clone());
    let vt_tx = VTTransaction::new(vt_body, vec![]);

    let x = validate_genesis_vt_transaction(&vt_tx);

    assert_eq!(x.unwrap(), (vec![&vto0], value));

    assert_eq!(vt_tx, VTTransaction::genesis(outputs));
}

#[test]
fn data_request_no_inputs() {
    let mut signatures_to_verify = vec![];
    let utxo_pool = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);

    // Try to create a data request with no inputs
    let dr_output = DataRequestOutput {
        witness_reward: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![], vec![], dr_output);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![]);
    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee
    );
}

#[test]
fn data_request_no_inputs_but_one_signature() {
    let mut signatures_to_verify = vec![];
    let utxo_pool = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);

    // No inputs but 1 signature
    let dr_output = DataRequestOutput {
        witness_reward: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![], vec![], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);
    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    );
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
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let dr_output = DataRequestOutput {
        witness_reward: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);

    let dr_transaction = DRTransaction::new(dr_tx_body, vec![]);

    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    );
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
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let dr_output = DataRequestOutput {
        witness_reward: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);

    test_signature_empty_wrong_bad(dr_tx_body, |dr_tx_body, drs| {
        let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);
        let mut signatures_to_verify = vec![];

        validate_dr_transaction(
            &dr_transaction,
            &utxo_diff,
            Epoch::default(),
            EpochConstants::default(),
            &mut signatures_to_verify,
            ONE_WIT,
        )?;
        verify_signatures_test(signatures_to_verify)?;

        Ok(())
    });
}

#[test]
fn data_request_input_not_in_utxo() {
    let mut signatures_to_verify = vec![];
    let utxo_pool = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(
        "2222222222222222222222222222222222222222222222222222222222222222:0"
            .parse()
            .unwrap(),
    );

    let dr_output = DataRequestOutput {
        witness_reward: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);
    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    );
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
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());

    let dr_output = DataRequestOutput {
        witness_reward: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);
    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee
    );
}

#[test]
fn data_request_output_value_overflow() {
    let mut signatures_to_verify = vec![];
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: u64::max_value() - 1_000,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti0 = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let vti1 = Input::new(utxo_pool.iter().nth(1).unwrap().0.clone());

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: u64::max_value() - 1_000,
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000,
        time_lock: 0,
    };

    // The sum of the value of vto0 + vto1 should not overflow,
    // but the sum of vto0 + vto1 + dr_output should overflow
    assert_eq!(
        transaction_outputs_sum(&[vto0.clone(), vto1.clone()]).unwrap(),
        u64::max_value()
    );

    let dr_output = DataRequestOutput {
        witness_reward: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti0, vti1], vec![vto0, vto1], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs; 2]);
    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::OutputValueOverflow
    );
}

// Helper function which creates a data request with a valid input with value 1000
// and returns the validation error
fn test_drtx(dr_output: DataRequestOutput) -> Result<(), failure::Error> {
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    )
    .map(|_| ())
}

fn test_rad_request(data_request: RADRequest) -> Result<(), failure::Error> {
    test_drtx(DataRequestOutput {
        witness_reward: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
        ..DataRequestOutput::default()
    })
}

// Example data request used in tests. It consists of just empty arrays.
// If this data request is modified, the `data_request_empty_scripts` test
// should be updated accordingly.
fn example_data_request() -> RADRequest {
    RADRequest {
        time_lock: 0,
        retrieve: vec![RADRetrieve {
            kind: RADType::HttpGet,
            url: "".to_string(),
            script: vec![0x80],
        }],
        aggregate: RADAggregate {
            filters: vec![],
            reducer: RadonReducers::Mode as u32,
        },
        tally: RADTally {
            filters: vec![],
            reducer: RadonReducers::Mode as u32,
        },
    }
}

fn example_data_request_with_mode_filter() -> RADRequest {
    RADRequest {
        time_lock: 0,
        retrieve: vec![RADRetrieve {
            kind: RADType::HttpGet,
            url: "".to_string(),
            script: vec![0x80],
        }],
        aggregate: RADAggregate {
            filters: vec![],
            reducer: RadonReducers::Mode as u32,
        },
        tally: RADTally {
            filters: vec![RADFilter {
                op: RadonFilters::Mode as u32,
                args: vec![],
            }],
            reducer: RadonReducers::Mode as u32,
        },
    }
}

#[test]
fn data_request_no_scripts() {
    let x = test_rad_request(RADRequest {
        time_lock: 0,
        retrieve: vec![],
        aggregate: RADAggregate::default(),
        tally: RADTally::default(),
    });
    assert_eq!(
        x.unwrap_err().downcast::<RadError>().unwrap(),
        RadError::UnsupportedReducerInAT { operator: 0 }
    );
}

#[test]
fn data_request_empty_scripts() {
    // 0x90 is an empty array in MessagePack
    let x = test_rad_request(example_data_request());

    // This is currently accepted as a valid data request.
    // If this test fails in the future, modify it to check that
    // this is an invalid data request.
    x.unwrap();
}

#[test]
fn data_request_witnesses_0() {
    // A data request with 0 witnesses is invalid
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        witness_reward: 500,
        witnesses: 0,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
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
        witness_reward: 1000,
        witnesses: 1,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
        ..DataRequestOutput::default()
    });
    x.unwrap();
}

#[test]
fn data_request_no_value() {
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        witness_reward: 0,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NoReward,
    );
}

#[test]
fn data_request_insufficient_collateral() {
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        witness_reward: 10,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: 1000,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidCollateral {
            value: 1000,
            min: ONE_WIT
        },
    );
}

#[test]
fn data_request_minimum_value() {
    // Create a data request with the minimum possible value
    let data_request = example_data_request();
    let dro = DataRequestOutput {
        witness_reward: 1,
        witnesses: 1,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
        ..DataRequestOutput::default()
    };
    // The dro is valid
    test_drtx(dro.clone()).unwrap();
    // The total value is 1
    assert_eq!(dro.checked_total_value(), Ok(1));
}

#[test]
fn data_request_no_reward() {
    let data_request = example_data_request();
    let x = test_drtx(DataRequestOutput {
        witness_reward: 0,
        commit_fee: 100,
        reveal_fee: 100,
        tally_fee: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
        ..DataRequestOutput::default()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NoReward,
    );
}

#[test]
fn data_request_value_overflow() {
    let data_request = example_data_request();
    let dro = DataRequestOutput {
        witness_reward: 1,
        commit_fee: 1,
        reveal_fee: 1,
        tally_fee: 1,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
        ..DataRequestOutput::default()
    };
    // Test different combinations of overflowing values
    let x = test_drtx(DataRequestOutput {
        witness_reward: u64::max_value(),
        ..dro.clone()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::FeeOverflow,
    );
    let x = test_drtx(DataRequestOutput {
        witness_reward: u64::max_value() / u64::from(u16::max_value()),
        witnesses: u16::max_value(),
        ..dro.clone()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::FeeOverflow,
    );
    let x = test_drtx(DataRequestOutput {
        commit_fee: u64::max_value(),
        ..dro.clone()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::FeeOverflow,
    );
    let x = test_drtx(DataRequestOutput {
        reveal_fee: u64::max_value(),
        ..dro.clone()
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::FeeOverflow,
    );
    let x = test_drtx(DataRequestOutput {
        tally_fee: u64::max_value(),
        ..dro
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::FeeOverflow,
    );
}

#[test]
fn data_request_miner_fee() {
    // Use 1000 input to pay 750 for data request
    let mut signatures_to_verify = vec![];
    let data_request = example_data_request();
    let dr_output = DataRequestOutput {
        witness_reward: 750 / 2,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
        ..DataRequestOutput::default()
    };

    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let dr_miner_fee = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    )
    .map(|(_, _, fee)| fee)
    .unwrap();
    assert_eq!(dr_miner_fee, 1000 - 750);
}

#[test]
fn data_request_miner_fee_with_change() {
    // Use 1000 input to pay 750 for data request, and request 200 change (+50 fee)
    let mut signatures_to_verify = vec![];
    let data_request = example_data_request();
    let dr_output = DataRequestOutput {
        witness_reward: 750 / 2,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
        ..DataRequestOutput::default()
    };

    let vto = ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let change_output = ValueTransferOutput {
        time_lock: 0,
        pkh: PublicKeyHash::default(),
        value: 200,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![change_output], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let dr_miner_fee = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    )
    .map(|(_, _, fee)| fee)
    .unwrap();
    assert_eq!(dr_miner_fee, 1000 - 750 - 200);
}

#[test]
fn data_request_miner_fee_with_too_much_change() {
    // Use 1000 input to pay 750 for data request, and request 300 change (-50 fee)
    let mut signatures_to_verify = vec![];
    let data_request = example_data_request();
    let dr_output = DataRequestOutput {
        witness_reward: 750 / 2,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
        ..DataRequestOutput::default()
    };

    let vto = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let change_output = ValueTransferOutput {
        time_lock: 0,
        pkh: PublicKeyHash::default(),
        value: 300,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![change_output], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee
    );
}

#[test]
fn data_request_zero_value_output() {
    // Use 1000 input to pay 750 for data request, and request 300 change (-50 fee)
    let mut signatures_to_verify = vec![];
    let data_request = example_data_request();
    let dr_output = DataRequestOutput {
        witness_reward: 750 / 2,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
        ..DataRequestOutput::default()
    };

    let vto = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH.parse().unwrap(),
        value: 1000,
    };
    let change_output = ValueTransferOutput {
        time_lock: 0,
        pkh: PublicKeyHash::default(),
        value: 0,
    };
    let utxo_pool = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_pool, block_number);
    let vti = Input::new(utxo_pool.iter().next().unwrap().0.clone());
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![change_output], dr_output);
    let drs = sign_t(&dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
    );
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
    let mut signatures_to_verify = vec![];
    let dr_pool = DataRequestPool::default();
    let beacon = CheckpointBeacon::default();
    let rep_eng = ReputationEngine::new(100);

    validate_commit_transaction(
        &c_tx,
        &dr_pool,
        beacon,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
    )
    .map(|_| ())
}

static DR_HASH: &str = "2b3e5252d9266d5bc62666052e9a6a8b00167c04a2339c3929acd62aee5aa4f4";

// Helper function to test a commit with an empty state (no utxos, no drs, etc)
fn test_commit_with_dr(c_tx: &CommitTransaction) -> Result<(), failure::Error> {
    let mut dr_pool = DataRequestPool::default();
    let commit_beacon = CheckpointBeacon::default();
    let rep_eng = ReputationEngine::new(100);

    let dro = DataRequestOutput {
        witness_reward: 1000,
        witnesses: 1,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    assert_eq!(dr_hash, DR_HASH.parse().unwrap());
    let dr_epoch = 0;
    dr_pool
        .process_data_request(
            &dr_transaction,
            dr_epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();

    let mut signatures_to_verify = vec![];
    validate_commit_transaction(
        &c_tx,
        &dr_pool,
        commit_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
    )?;
    verify_signatures_test(signatures_to_verify)?;

    Ok(())
}

// Helper function to test a commit with an existing data request,
// but it is very difficult to construct a valid vrf proof
fn test_commit_difficult_proof() {
    let mut dr_pool = DataRequestPool::default();
    let commit_beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let secret_key = SecretKey {
        bytes: Protected::from(vec![0xcd; 32]),
    };

    // Create a reputation engine where one identity has 1_023 reputation,
    // so it is very difficult for someone with 0 reputation to be elegible
    // for a data request
    let mut rep_eng = ReputationEngine::new(100);
    let rep_pkh = PublicKeyHash::default();
    rep_eng
        .trs_mut()
        .gain(Alpha(1000), vec![(rep_pkh, Reputation(1_023))])
        .unwrap();
    rep_eng.ars_mut().push_activity(vec![rep_pkh]);

    let dro = DataRequestOutput {
        witness_reward: 1000,
        witnesses: 1,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    assert_eq!(dr_hash, DR_HASH.parse().unwrap());
    let dr_epoch = 0;
    dr_pool
        .process_data_request(
            &dr_transaction,
            dr_epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof =
        DataRequestEligibilityClaim::create(vrf, &secret_key, commit_beacon, dr_hash).unwrap();
    // Sign commitment
    let cs = sign_t(&cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    let mut signatures_to_verify = vec![];
    let x = validate_commit_transaction(
        &c_tx,
        &dr_pool,
        commit_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
    )
    .and_then(|_| verify_signatures_test(signatures_to_verify));

    match x.unwrap_err().downcast::<TransactionError>().unwrap() {
        TransactionError::DataRequestEligibilityDoesNotMeetTarget { target_hash, .. }
            if target_hash == Hash::with_first_u32(0x003f_ffff) => {}
        e => panic!("{:?}", e),
    }
}

// Helper function to test a commit with an existing data request
fn test_commit() -> Result<(), failure::Error> {
    let mut signatures_to_verify = vec![];
    let mut dr_pool = DataRequestPool::default();
    let commit_beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let secret_key = SecretKey {
        bytes: Protected::from(vec![0xcd; 32]),
    };

    let dro = DataRequestOutput {
        witness_reward: 1000,
        witnesses: 1,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    assert_eq!(dr_hash, DR_HASH.parse().unwrap());
    let dr_epoch = 0;
    dr_pool
        .process_data_request(
            &dr_transaction,
            dr_epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof =
        DataRequestEligibilityClaim::create(vrf, &secret_key, commit_beacon, dr_hash).unwrap();
    // Sign commitment
    let cs = sign_t(&cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    validate_commit_transaction(
        &c_tx,
        &dr_pool,
        commit_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
    )
    .map(|_| ())
}

#[test]
fn commitment_signatures() {
    let dr_hash = DR_HASH.parse().unwrap();
    let commit_beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let secret_key = SecretKey {
        bytes: Protected::from(vec![0xcd; 32]),
    };
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
            msg: "Fail in verify process".to_string(),
        },
    );

    // Flip one bit in the public key of the signature
    let mut ks_bad_pk = ks;
    ks_bad_pk.public_key.bytes[13] ^= 0x01;
    let signature_pkh = ks_bad_pk.public_key.pkh();
    let x = f(hashable.clone(), ks_bad_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::PublicKeyHashMismatch {
            expected_pkh: MY_PKH.parse().unwrap(),
            signature_pkh,
        }
    );

    // Sign transaction with a different public key
    let ks_different_pk = sign_t2(&hashable);
    let signature_pkh = ks_different_pk.public_key.pkh();
    let x = f(hashable, ks_different_pk);
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
    let secret_key = SecretKey {
        bytes: Protected::from(vec![0xcd; 32]),
    };

    // Create an invalid proof by suppliying a different dr_pointer
    let bad_dr_pointer = Hash::default();
    cb.proof = DataRequestEligibilityClaim::create(vrf, &secret_key, commit_beacon, bad_dr_pointer)
        .unwrap();

    let dro = DataRequestOutput {
        witness_reward: 1000,
        witnesses: 1,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_epoch = 0;
    dr_pool
        .process_data_request(
            &dr_transaction,
            dr_epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();

    // Sign commitment
    let cs = sign_t(&cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);
    let mut signatures_to_verify = vec![];

    let x = validate_commit_transaction(
        &c_tx,
        &dr_pool,
        commit_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
    )
    .and_then(|_| verify_signatures_test(signatures_to_verify));

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidDataRequestPoe,
    );
}

#[test]
fn commitment_proof_lower_than_target() {
    test_commit_difficult_proof();
}

#[test]
fn commitment_dr_in_reveal_stage() {
    let mut dr_pool = DataRequestPool::default();
    let block_hash = Hash::default();
    let commit_beacon = CheckpointBeacon::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let secret_key = SecretKey {
        bytes: Protected::from(vec![0xcd; 32]),
    };

    let dro = DataRequestOutput {
        witness_reward: 1000,
        witnesses: 1,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    let dr_epoch = 0;
    dr_pool
        .process_data_request(
            &dr_transaction,
            dr_epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();
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
    let mut signatures_to_verify = vec![];

    let x = validate_commit_transaction(
        &c_tx,
        &dr_pool,
        commit_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
    );
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NotCommitStage,
    );
}

#[test]
fn commitment_valid() {
    let x = test_commit();
    x.unwrap();
}

#[test]
fn commitment_timelock() {
    // 1 epoch = 1000 seconds, for easy testing
    let epoch_constants = EpochConstants {
        checkpoint_zero_timestamp: 0,
        checkpoints_period: 1_000,
    };
    let test_commit_epoch = |epoch, time_lock| {
        let mut dr_pool = DataRequestPool::default();
        let commit_beacon = CheckpointBeacon::default();
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let rep_eng = ReputationEngine::new(100);
        let secret_key = SecretKey {
            bytes: Protected::from(vec![0xcd; 32]),
        };

        let mut rad_request = example_data_request();
        rad_request.time_lock = time_lock;

        let dro = DataRequestOutput {
            witness_reward: 1000,
            witnesses: 1,
            min_consensus_percentage: 51,
            data_request: rad_request,
            collateral: ONE_WIT,
            ..DataRequestOutput::default()
        };
        let dr_body = DRTransactionBody::new(vec![], vec![], dro);
        let drs = sign_t(&dr_body);
        let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
        let dr_hash = dr_transaction.hash();
        let dr_epoch = 0;
        dr_pool
            .process_data_request(
                &dr_transaction,
                dr_epoch,
                EpochConstants::default(),
                &Hash::default(),
            )
            .unwrap();

        // Insert valid proof
        let mut cb = CommitTransactionBody::default();
        cb.dr_pointer = dr_hash;
        cb.proof =
            DataRequestEligibilityClaim::create(vrf, &secret_key, commit_beacon, dr_hash).unwrap();
        // Sign commitment
        let cs = sign_t(&cb);
        let c_tx = CommitTransaction::new(cb, vec![cs]);

        let mut signatures_to_verify = vec![];
        validate_commit_transaction(
            &c_tx,
            &dr_pool,
            commit_beacon,
            &mut signatures_to_verify,
            &rep_eng,
            epoch,
            epoch_constants,
        )
        .map(|_| ())?;

        verify_signatures_test(signatures_to_verify)
    };

    // (epoch, time_lock, should_be_accepted_into_block)
    let tests = vec![
        (0, 0, true),
        (0, 1, false),
        (0, 1_000_000, false),
        (999, 1_000_000, false),
        (999, 999_999, false),
        (1000, 999_999, true),
        (1000, 1_000_000, true),
        (1000, 1_000_001, false),
        (1001, 1_000_000, true),
        (1001, 1_000_001, true),
    ];

    for (epoch, time_lock, is_ok) in tests {
        let x = test_commit_epoch(epoch, time_lock);
        assert_eq!(x.is_ok(), is_ok, "{:?}: {:?}", (epoch, time_lock, is_ok), x);
    }
}

fn dr_pool_with_dr_in_reveal_stage() -> (DataRequestPool, Hash) {
    let mut dr_pool = DataRequestPool::default();
    let block_hash = Hash::default();
    let epoch = 0;
    let dro = DataRequestOutput {
        witness_reward: 1000,
        witnesses: 1,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
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

    dr_pool
        .process_data_request(
            &dr_transaction,
            epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();
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

    let f = |rb, rs| -> Result<_, failure::Error> {
        let r_tx = RevealTransaction::new(rb, vec![rs]);
        let mut signatures_to_verify = vec![];
        let ret = validate_reveal_transaction(&r_tx, &dr_pool, &mut signatures_to_verify)?;
        verify_signatures_test(signatures_to_verify)?;
        Ok(ret)
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
        TransactionError::MismatchedCommitment,
    );

    // Flip one bit in the public key of the signature
    let mut ks_bad_pk = ks;
    ks_bad_pk.public_key.bytes[13] ^= 0x01;
    let signature_pkh = ks_bad_pk.public_key.pkh();

    let x = f(hashable.clone(), ks_bad_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::PublicKeyHashMismatch {
            expected_pkh: MY_PKH.parse().unwrap(),
            signature_pkh,
        }
    );

    // Sign transaction with a different public key
    let ks_different_pk = sign_t2(&hashable);
    let signature_pkh = ks_different_pk.public_key.pkh();
    let x = f(hashable, ks_different_pk);
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
    let mut signatures_to_verify = vec![];
    let mut dr_pool = DataRequestPool::default();
    let epoch = 0;
    let dro = DataRequestOutput {
        witness_reward: 1000,
        witnesses: 1,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_pointer = dr_transaction.hash();
    dr_pool
        .add_data_request(epoch, dr_transaction, &Hash::default())
        .unwrap();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    let rs = sign_t(&rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool, &mut signatures_to_verify);
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NotRevealStage,
    );
}

#[test]
fn reveal_no_signature() {
    let mut signatures_to_verify = vec![];
    let (dr_pool, dr_pointer) = dr_pool_with_dr_in_reveal_stage();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    let r_tx = RevealTransaction::new(rb, vec![]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool, &mut signatures_to_verify);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::SignatureNotFound,
    );
}

#[test]
fn reveal_wrong_signature_public_key() {
    let mut signatures_to_verify = vec![];
    let (dr_pool, dr_pointer) = dr_pool_with_dr_in_reveal_stage();

    let bad_pkh = PublicKeyHash::default();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    rb.pkh = bad_pkh;
    let rs = sign_t(&rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool, &mut signatures_to_verify);
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
    let mut signatures_to_verify = vec![];
    let dr_pool = DataRequestPool::default();
    let dr_pointer = "2222222222222222222222222222222222222222222222222222222222222222"
        .parse()
        .unwrap();
    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    let rs = sign_t(&rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool, &mut signatures_to_verify);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DataRequestNotFound { hash: dr_pointer },
    );
}

#[test]
fn reveal_no_commitment() {
    let mut signatures_to_verify = vec![];
    let (dr_pool, dr_pointer) = dr_pool_with_dr_in_reveal_stage();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    rb.pkh = MY_PKH_2.parse().unwrap();
    let rs = sign_t2(&rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool, &mut signatures_to_verify);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::CommitNotFound,
    );
}

#[test]
fn reveal_invalid_commitment() {
    let mut signatures_to_verify = vec![];
    let (dr_pool, dr_pointer) = dr_pool_with_dr_in_reveal_stage();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    rb.pkh = MY_PKH.parse().unwrap();
    let rs = sign_t(&rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool, &mut signatures_to_verify);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchedCommitment,
    );
}

#[test]
fn reveal_valid_commitment() {
    let mut signatures_to_verify = vec![];
    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 5,
        reveal_fee: 20,
        extra_reveal_rounds: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_transaction = DRTransaction {
        body: DRTransactionBody::new(vec![], vec![], dr_output),
        signatures: vec![KeyedSignature::default()],
    };
    let dr_pointer = dr_transaction.hash();

    // Include DRTransaction in DataRequestPool
    dr_pool
        .process_data_request(
            &dr_transaction,
            epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();
    dr_pool.update_data_request_stages();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key;

    // Create Reveal and Commit
    let reveal_body = RevealTransactionBody::new(dr_pointer, vec![], public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::without_collateral(
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

    let fee = validate_reveal_transaction(&reveal_transaction, &dr_pool, &mut signatures_to_verify)
        .unwrap();
    assert_eq!(fee, 20);

    // Create other reveal
    let reveal_body2 = RevealTransactionBody::new(
        dr_pointer,
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
        public_key.pkh(),
    );
    let reveal_signature2 = sign_t(&reveal_body2);
    let reveal_transaction2 = RevealTransaction::new(reveal_body2, vec![reveal_signature2]);

    let error =
        validate_reveal_transaction(&reveal_transaction2, &dr_pool, &mut signatures_to_verify);
    assert_eq!(
        error.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchedCommitment
    );

    // Include RevealTransaction in DataRequestPool
    dr_pool
        .process_reveal(&reveal_transaction, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    // Validate trying to include a reveal previously included
    let error =
        validate_reveal_transaction(&reveal_transaction, &dr_pool, &mut signatures_to_verify);
    assert_eq!(
        error.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DuplicatedReveal {
            pkh: public_key.pkh(),
            dr_pointer,
        }
    );
}

// In this setup we create a DataRequestPool with a Data Request in Tally stage
// with 5 commits but a only 1 reveal
fn dr_pool_with_dr_in_tally_stage(
    reveal_value: Vec<u8>,
) -> (
    DataRequestPool,
    Hash,
    PublicKeyHash,
    PublicKeyHash,
    Vec<PublicKeyHash>,
) {
    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 5,
        reveal_fee: 20,
        witness_reward: 1000 / 5,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_transaction_body = DRTransactionBody::new(vec![], vec![], dr_output);
    let dr_transaction_signature = sign_t2(&dr_transaction_body);
    let dr_pkh = dr_transaction_signature.public_key.pkh();
    let dr_transaction = DRTransaction::new(dr_transaction_body, vec![dr_transaction_signature]);
    let dr_pointer = dr_transaction.hash();

    // Include DRTransaction in DataRequestPool
    dr_pool
        .process_data_request(
            &dr_transaction,
            epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();
    dr_pool.update_data_request_stages();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key;

    // Create Reveal and Commit
    // Reveal = empty array
    let reveal_body = RevealTransactionBody::new(dr_pointer, reveal_value, public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let reveal_transaction = RevealTransaction::new(reveal_body, vec![reveal_signature]);
    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::without_collateral(
            dr_pointer,
            commitment,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key: public_key.clone(),
        }],
    );

    // We create 4 commits more with different PublicKey and the same response
    let pk2 = PublicKey {
        compressed: 0,
        bytes: [2; 32],
    };
    let pk3 = PublicKey {
        compressed: 0,
        bytes: [3; 32],
    };
    let pk4 = PublicKey {
        compressed: 0,
        bytes: [4; 32],
    };
    let pk5 = PublicKey {
        compressed: 0,
        bytes: [5; 32],
    };

    let mut commit_transaction2 = commit_transaction.clone();
    commit_transaction2.signatures[0].public_key = pk2.clone();
    let mut commit_transaction3 = commit_transaction.clone();
    commit_transaction3.signatures[0].public_key = pk3.clone();
    let mut commit_transaction4 = commit_transaction.clone();
    commit_transaction4.signatures[0].public_key = pk4.clone();
    let mut commit_transaction5 = commit_transaction.clone();
    commit_transaction5.signatures[0].public_key = pk5.clone();

    // Include the 5 CommitTransactions in DataRequestPool
    dr_pool
        .process_commit(&commit_transaction, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction2, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction3, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction4, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction5, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    // Include the unique RevealTransaction in DataRequestPool
    dr_pool
        .process_reveal(&reveal_transaction, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    (
        dr_pool,
        dr_pointer,
        public_key.pkh(),
        dr_pkh,
        vec![pk2.pkh(), pk3.pkh(), pk4.pkh(), pk5.pkh()],
    )
}

// In this setup we create a DataRequestPool with a Data Request in Tally stage
// but in the special case of 0 commits
fn dr_pool_with_dr_in_tally_stage_no_commits(
) -> (DataRequestPool, Hash, DataRequestOutput, PublicKeyHash) {
    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create DRTransaction
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 5,
        commit_fee: 20,
        reveal_fee: 20,
        tally_fee: 100,
        witness_reward: 200,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };

    assert_eq!(dr_output.extra_commit_rounds, 0);

    let dr_transaction_body = DRTransactionBody::new(vec![], vec![], dr_output.clone());
    let dr_transaction_signature = sign_t2(&dr_transaction_body);
    let dr_pkh = dr_transaction_signature.public_key.pkh();
    let dr_transaction = DRTransaction::new(dr_transaction_body, vec![dr_transaction_signature]);
    let dr_pointer = dr_transaction.hash();

    // Include DRTransaction in DataRequestPool
    dr_pool
        .process_data_request(
            &dr_transaction,
            epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();
    dr_pool.update_data_request_stages();

    // Zero extra commits rounds and no commits in one round move the data request
    // from COMMIT stage to TALLY
    dr_pool.update_data_request_stages();
    assert_eq!(
        dr_pool.data_request_pool[&dr_pointer].stage,
        DataRequestStage::TALLY
    );

    (dr_pool, dr_pointer, dr_output, dr_pkh)
}

// In this setup we create a DataRequestPool with a Data Request in Tally stage
// but in the special case of 0 reveals
// In case of no reveals, all of the committers are slashed
fn dr_pool_with_dr_in_tally_stage_no_reveals() -> (
    DataRequestPool,
    Hash,
    DataRequestOutput,
    PublicKeyHash,
    Vec<PublicKeyHash>,
) {
    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 5,
        commit_fee: 20,
        reveal_fee: 20,
        tally_fee: 100,
        witness_reward: 200,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };

    assert_eq!(dr_output.extra_reveal_rounds, 0);

    let dr_transaction_body = DRTransactionBody::new(vec![], vec![], dr_output.clone());
    let dr_transaction_signature = sign_t2(&dr_transaction_body);
    let dr_pkh = dr_transaction_signature.public_key.pkh();
    let dr_transaction = DRTransaction::new(dr_transaction_body, vec![dr_transaction_signature]);
    let dr_pointer = dr_transaction.hash();

    // Include DRTransaction in DataRequestPool
    dr_pool
        .process_data_request(
            &dr_transaction,
            epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();
    dr_pool.update_data_request_stages();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key;

    // Create Reveal and Commit
    // Reveal = empty array
    let reveal_body = RevealTransactionBody::new(dr_pointer, [0x00].to_vec(), public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::without_collateral(
            dr_pointer,
            commitment,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key: public_key.clone(),
        }],
    );

    // We create 4 commits more with different PublicKey and the same response
    let pk2 = PublicKey {
        compressed: 0,
        bytes: [2; 32],
    };
    let pk3 = PublicKey {
        compressed: 0,
        bytes: [3; 32],
    };
    let pk4 = PublicKey {
        compressed: 0,
        bytes: [4; 32],
    };
    let pk5 = PublicKey {
        compressed: 0,
        bytes: [5; 32],
    };

    let mut commit_transaction2 = commit_transaction.clone();
    commit_transaction2.signatures[0].public_key = pk2.clone();
    let mut commit_transaction3 = commit_transaction.clone();
    commit_transaction3.signatures[0].public_key = pk3.clone();
    let mut commit_transaction4 = commit_transaction.clone();
    commit_transaction4.signatures[0].public_key = pk4.clone();
    let mut commit_transaction5 = commit_transaction.clone();
    commit_transaction5.signatures[0].public_key = pk5.clone();

    // Include the 5 CommitTransactions in DataRequestPool
    dr_pool
        .process_commit(&commit_transaction, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction2, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction3, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction4, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction5, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    // Zero extra reveal rounds and no reveals in one round move the data request
    // from REVEAL stage to TALLY
    dr_pool.update_data_request_stages();
    assert_eq!(
        dr_pool.data_request_pool[&dr_pointer].stage,
        DataRequestStage::TALLY
    );

    (
        dr_pool,
        dr_pointer,
        dr_output,
        dr_pkh,
        vec![public_key.pkh(), pk2.pkh(), pk3.pkh(), pk4.pkh(), pk5.pkh()],
    )
}

// In this setup we create a DataRequestPool with a Data Request in Tally stage
// with 2 commits and 2 reveals
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
        reveal_fee: 50,
        commit_fee: 50,
        tally_fee: 100,
        witness_reward: 1300 / 2,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_transaction = DRTransaction {
        body: DRTransactionBody::new(vec![], vec![], dr_output),
        signatures: vec![KeyedSignature::default()],
    };
    let dr_pointer = dr_transaction.hash();

    // Include DRTransaction in DataRequestPool
    dr_pool
        .process_data_request(
            &dr_transaction,
            epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();
    dr_pool.update_data_request_stages();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key;
    let public_key2 = sign_t2(&RevealTransactionBody::default()).public_key;

    // Create Reveal and Commit
    // Reveal = empty array
    let reveal_body =
        RevealTransactionBody::new(dr_pointer, reveal_value.clone(), public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::without_collateral(
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
        CommitTransactionBody::without_collateral(
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

    // Include CommitTransactions in DataRequestPool
    dr_pool
        .process_commit(&commit_transaction, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction2, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    // Include RevealTransactions in DataRequestPool
    dr_pool
        .process_reveal(&reveal_transaction, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_reveal(&reveal_transaction2, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    (dr_pool, dr_pointer, public_key.pkh(), public_key2.pkh())
}

// In this setup we create a DataRequestPool with a Data Request in Tally stage
// with 3 commits and 3 reveals, one of the committers is the data requester and he is lying
fn dr_pool_with_dr_in_tally_stage_3_reveals_data_requester_lie(
    reveal_value: Vec<u8>,
    liar_value: Vec<u8>,
) -> (
    DataRequestPool,
    DataRequestOutput,
    Hash,
    PublicKeyHash,
    PublicKeyHash,
    PublicKeyHash,
) {
    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key;
    let public_key2 = sign_t2(&RevealTransactionBody::default()).public_key;
    let dr_public_key = sign_t3(&RevealTransactionBody::default()).public_key;

    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 3,
        reveal_fee: 50,
        commit_fee: 50,
        tally_fee: 200,
        witness_reward: 500,
        min_consensus_percentage: 51,
        data_request: example_data_request_with_mode_filter(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_transaction = DRTransaction {
        body: DRTransactionBody::new(vec![], vec![], dr_output.clone()),
        signatures: vec![KeyedSignature {
            signature: Default::default(),
            public_key: dr_public_key.clone(),
        }],
    };
    let dr_pointer = dr_transaction.hash();

    // Include DRTransaction in DataRequestPool
    dr_pool
        .process_data_request(
            &dr_transaction,
            epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();
    dr_pool.update_data_request_stages();

    // Create Reveal and Commit
    // Reveal = empty array
    let reveal_body =
        RevealTransactionBody::new(dr_pointer, reveal_value.clone(), public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::without_collateral(
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
        CommitTransactionBody::without_collateral(
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

    // Reveal and commitment with the lie
    let reveal_body3 = RevealTransactionBody::new(dr_pointer, liar_value, dr_public_key.pkh());
    let reveal_signature3 = sign_t3(&reveal_body3);
    let commitment3 = reveal_signature3.signature.hash();

    let commit_transaction3 = CommitTransaction::new(
        CommitTransactionBody::without_collateral(
            dr_pointer,
            commitment3,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key: dr_public_key.clone(),
        }],
    );
    let reveal_transaction3 = RevealTransaction::new(reveal_body3, vec![reveal_signature3]);

    // Include the CommitTransactions in DataRequestPool
    dr_pool
        .process_commit(&commit_transaction, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction2, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction3, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    // Include the RevealTransactions in DataRequestPool
    dr_pool
        .process_reveal(&reveal_transaction, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_reveal(&reveal_transaction2, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_reveal(&reveal_transaction3, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    (
        dr_pool,
        dr_output,
        dr_pointer,
        public_key.pkh(),
        public_key2.pkh(),
        dr_public_key.pkh(),
    )
}

#[test]
fn tally_dr_not_tally_stage() {
    // Check that data request exists and is in tally stage

    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 5,
        reveal_fee: 20,
        witness_reward: 1000 / 5,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_transaction_body = DRTransactionBody::new(vec![], vec![], dr_output);
    let dr_transaction_signature = sign_t2(&dr_transaction_body);
    let dr_pkh = dr_transaction_signature.public_key.pkh();
    let dr_transaction = DRTransaction::new(dr_transaction_body, vec![dr_transaction_signature]);
    let dr_pointer = dr_transaction.hash();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key;

    // Create Reveal and Commit
    // Reveal = integer(0)
    let reveal_value = vec![0x00];
    let reveal_body = RevealTransactionBody::new(dr_pointer, reveal_value, public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::without_collateral(
            dr_pointer,
            commitment,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key: public_key.clone(),
        }],
    );

    let pk2 = PublicKey {
        compressed: 0,
        bytes: [2; 32],
    };
    let pk3 = PublicKey {
        compressed: 0,
        bytes: [3; 32],
    };
    let pk4 = PublicKey {
        compressed: 0,
        bytes: [4; 32],
    };
    let pk5 = PublicKey {
        compressed: 0,
        bytes: [5; 32],
    };

    let mut commit_transaction2 = commit_transaction.clone();
    commit_transaction2.signatures[0].public_key = pk2.clone();
    let mut commit_transaction3 = commit_transaction.clone();
    commit_transaction3.signatures[0].public_key = pk3.clone();
    let mut commit_transaction4 = commit_transaction.clone();
    commit_transaction4.signatures[0].public_key = pk4.clone();
    let mut commit_transaction5 = commit_transaction.clone();
    commit_transaction5.signatures[0].public_key = pk5.clone();

    let reveal_transaction = RevealTransaction::new(reveal_body, vec![reveal_signature]);
    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: public_key.pkh(),
        value: 200,
    };
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: 880,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt_change],
        vec![pk2.pkh(), pk3.pkh(), pk4.pkh(), pk5.pkh()],
    );

    let mut dr_pool = DataRequestPool::default();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DataRequestNotFound { hash: dr_pointer },
    );
    dr_pool
        .process_data_request(
            &dr_transaction,
            epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();
    dr_pool.update_data_request_stages();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool);
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NotTallyStage
    );

    dr_pool
        .process_commit(&commit_transaction, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction2, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction3, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction4, &fake_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction5, &fake_block_hash)
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
    x.unwrap();
}

#[test]
fn tally_invalid_consensus() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh, _dr_pkh, _) = dr_pool_with_dr_in_tally_stage(reveal_value);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    // Fake tally value: integer(1)
    let fake_tally_value = vec![0x01];

    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 200,
    };
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: PublicKeyHash::default(),
        value: 800,
    };

    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        fake_tally_value.clone(),
        vec![vt0, vt_change],
        vec![],
    );
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
    let (dr_pool, dr_pointer, pkh, dr_pkh, slashed_pkhs) =
        dr_pool_with_dr_in_tally_stage(reveal_value);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 200,
    };
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: 880,
    };
    let tally_transaction =
        TallyTransaction::new(dr_pointer, tally_value, vec![vt0, vt_change], slashed_pkhs);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    x.unwrap();
}

#[test]
fn tally_too_many_outputs() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh, dr_pkh, slashed_pkhs) =
        dr_pool_with_dr_in_tally_stage(reveal_value);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 200,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 200,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 200,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 200,
    };
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: 800,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt_change],
        slashed_pkhs,
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

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 500,
    };

    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value, vec![vt0], vec![]);
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
    let (dr_pool, dr_pointer, pkh, dr_pkh, slashed_pkhs) =
        dr_pool_with_dr_in_tally_stage(reveal_value);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 200,
    };
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: 1000,
    };
    let tally_transaction =
        TallyTransaction::new(dr_pointer, tally_value, vec![vt0, vt_change], slashed_pkhs);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidTallyChange {
            change: 1000,
            expected_change: 880
        },
    );
}

#[test]
fn tally_double_reward() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh, _pkh2) = dr_pool_with_dr_in_tally_stage_2_reveals(reveal_value);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 500,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 500,
    };
    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value, vec![vt0, vt1], vec![]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MultipleRewards { pkh },
    );
}

#[test]
fn tally_reveal_not_found() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh, _pkh2) = dr_pool_with_dr_in_tally_stage_2_reveals(reveal_value);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 500,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: PublicKeyHash::default(),
        value: 500,
    };
    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value, vec![vt0, vt1], vec![]);
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

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 500,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkh2,
        value: 500,
    };
    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value, vec![vt0, vt1], vec![]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_3_reveals_dr_liar() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let liar_value = vec![0x0a];
    let (dr_pool, dr_output, dr_pointer, pkh, pkh2, dr_pkh) =
        dr_pool_with_dr_in_tally_stage_3_reveals_data_requester_lie(reveal_value, liar_value);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: dr_output.witness_reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkh2,
        value: dr_output.witness_reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: dr_output.witness_reward,
    };
    let slashed_pkh = vec![dr_pkh];
    let tally_transaction =
        TallyTransaction::new(dr_pointer, tally_value, vec![vt0, vt1, vt2], slashed_pkh);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert!(x.is_ok());
}

#[test]
fn tally_valid_3_reveals_dr_liar_invalid() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let liar_value = vec![0x0a];
    let (dr_pool, dr_output, dr_pointer, pkh, pkh2, dr_pkh) =
        dr_pool_with_dr_in_tally_stage_3_reveals_data_requester_lie(reveal_value, liar_value);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: dr_output.witness_reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkh2,
        value: dr_output.witness_reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: dr_output.witness_reward,
    };
    let slashed_witnesses = vec![];
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2],
        slashed_witnesses,
    );
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchingSlashedWitnesses {
            expected: vec![].into_iter().sorted().collect(),
            found: vec![dr_pkh].into_iter().sorted().collect(),
        },
    );
}

#[test]
fn create_tally_validation_dr_liar() {
    let reveal_value = RadonReport::from_result(
        Ok(RadonTypes::from(RadonInteger::from(1))),
        &ReportContext::default(),
    );
    let liar_value = RadonReport::from_result(
        Ok(RadonTypes::from(RadonInteger::from(5))),
        &ReportContext::default(),
    );
    // Create a DataRequestPool with 3 reveals (one of them is a lie from the data requester)
    let (dr_pool, dr_output, dr_pointer, pkh, pkh2, dr_pkh) =
        dr_pool_with_dr_in_tally_stage_3_reveals_data_requester_lie(
            reveal_value.result.encode().unwrap(),
            liar_value.result.encode().unwrap(),
        );

    // Create the RadonReport using the reveals and the RADTally script
    let clause_result = evaluate_tally_precondition_clause(
        vec![reveal_value.clone(), reveal_value, liar_value],
        0.51,
        3,
    );
    let script = RADTally {
        filters: vec![RADFilter {
            op: RadonFilters::Mode as u32,
            args: vec![],
        }],
        reducer: RadonReducers::Mode as u32,
    };
    let report = construct_report_from_clause_result(clause_result, &script, 3);

    // Create a TallyTransaction using the create_tally function
    let tally_transaction = create_tally(
        dr_pointer,
        &dr_output,
        dr_pkh,
        &report,
        vec![pkh, pkh2, dr_pkh],
        vec![pkh, pkh2, dr_pkh]
            .iter()
            .cloned()
            .collect::<HashSet<PublicKeyHash>>(),
    )
    .unwrap();

    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert!(x.is_ok());
}

#[test]
fn tally_valid_zero_commits() {
    let (dr_pool, dr_pointer, dr_output, dr_pkh) = dr_pool_with_dr_in_tally_stage_no_commits();

    let change = (dr_output.commit_fee + dr_output.reveal_fee + dr_output.witness_reward)
        * u64::from(dr_output.witnesses);

    // Tally value: Insufficient commits Error
    let clause_result = evaluate_tally_precondition_clause(vec![], 0.0, 0);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0);
    let tally_value = report.result.encode().unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value, vec![vt0], vec![]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    x.unwrap();
}

#[test]
fn create_tally_validation_zero_commits() {
    let (dr_pool, dr_pointer, dr_output, dr_pkh) = dr_pool_with_dr_in_tally_stage_no_commits();

    // Tally value: Insufficient commits Error
    let clause_result = evaluate_tally_precondition_clause(vec![], 0.51, 0);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0);
    let tally_transaction = create_tally(
        dr_pointer,
        &dr_output,
        dr_pkh,
        &report,
        vec![],
        HashSet::<PublicKeyHash>::default(),
    )
    .unwrap();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    x.unwrap();
}

#[test]
fn tally_invalid_zero_commits() {
    let (dr_pool, dr_pointer, dr_output, dr_pkh) = dr_pool_with_dr_in_tally_stage_no_commits();

    let change = (dr_output.commit_fee + dr_output.reveal_fee + dr_output.witness_reward)
        * u64::from(dr_output.witnesses);

    // Tally value: Insufficient commits Error
    let clause_result = evaluate_tally_precondition_clause(vec![], 0.0, 0);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0);
    let tally_value = report.result.encode().unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: PublicKeyHash::default(),
        value: dr_output.witness_reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value, vec![vt0, vt1], vec![]);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::WrongNumberOutputs {
            outputs: 2,
            expected_outputs: 1
        },
    );
}

#[test]
fn tally_valid_zero_reveals() {
    let (dr_pool, dr_pointer, dr_output, dr_pkh, slashed_pkhs) =
        dr_pool_with_dr_in_tally_stage_no_reveals();

    let change = (dr_output.reveal_fee + dr_output.witness_reward) * u64::from(dr_output.witnesses);

    // Tally value: NoReveals commits Error
    let clause_result = evaluate_tally_precondition_clause(vec![], 0.51, 5);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0);
    let tally_value = report.result.encode().unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(dr_pointer, tally_value, vec![vt0], slashed_pkhs);
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    x.unwrap();
}

#[test]
fn create_tally_validation_zero_reveals() {
    let (dr_pool, dr_pointer, dr_output, dr_pkh, slashed_pkhs) =
        dr_pool_with_dr_in_tally_stage_no_reveals();

    // Tally value: NoReveals commits Error
    let clause_result = evaluate_tally_precondition_clause(vec![], 0.51, 5);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0);

    let tally_transaction = create_tally(
        dr_pointer,
        &dr_output,
        dr_pkh,
        &report,
        vec![],
        slashed_pkhs
            .iter()
            .cloned()
            .collect::<HashSet<PublicKeyHash>>(),
    )
    .unwrap();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool).map(|_| ());
    x.unwrap();
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
    let secret_key = SecretKey {
        bytes: Protected::from(vec![0xcd; 32]),
    };
    b.block_header.proof =
        BlockEligibilityClaim::create(vrf, &secret_key, b.block_header.beacon).unwrap();

    let hashable = b;
    let f = |mut b: Block, ks| -> Result<_, failure::Error> {
        b.block_sig = ks;
        let mut signatures_to_verify = vec![];
        validate_block_signature(&b, &mut signatures_to_verify)?;
        verify_signatures_test(signatures_to_verify)?;
        Ok(())
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
    let mut ks_bad_pk = ks;
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
    let x = f(hashable, ks_different_pk);
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

static BOOTSTRAP_HASH: &str = "4404291750b0cff95068e9894040e84e27cfdab1cb14f8c59928c3480a155b68";
static GENESIS_BLOCK_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";
static LAST_BLOCK_HASH: &str = "62adde3e36db3f22774cc255215b2833575f66bf2204011f80c03d34c7c9ea41";

fn test_block<F: FnMut(&mut Block) -> bool>(mut_block: F) -> Result<(), failure::Error> {
    test_block_with_drpool(mut_block, DataRequestPool::default())
}

fn test_block_with_drpool<F: FnMut(&mut Block) -> bool>(
    mut mut_block: F,
    dr_pool: DataRequestPool,
) -> Result<(), failure::Error> {
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let mut utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    // Insert output to utxo
    let output1 = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000_000,
    };
    //let tx_output1 = VTTransactionBody::new(vec![], vec![output1.clone()]);
    //let output1_pointer = OutputPointer { transaction_id: tx_output1.hash(), output_index: 0 };
    let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
    utxo_set.insert(output1_pointer, output1, block_number);

    let secret_key = SecretKey {
        bytes: Protected::from(vec![0xcd; 32]),
    };
    let current_epoch = 1000;
    let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let block_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let my_pkh = PublicKeyHash::default();
    let mining_bf = 8;
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let genesis_block_hash = GENESIS_BLOCK_HASH.parse().unwrap();

    let mut txns = BlockTransactions::default();
    txns.mint = MintTransaction::new(
        current_epoch,
        ValueTransferOutput {
            time_lock: 0,
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
    let mut signatures_to_verify = vec![];

    validate_candidate(
        &b,
        current_epoch,
        &mut signatures_to_verify,
        u32::try_from(rep_eng.ars().active_identities_number())?,
        mining_bf,
    )?;
    verify_signatures_test(signatures_to_verify)?;
    let mut signatures_to_verify = vec![];

    validate_block(
        &b,
        current_epoch,
        chain_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        mining_bf,
        bootstrap_hash,
        genesis_block_hash,
    )?;
    verify_signatures_test(signatures_to_verify)?;
    let mut signatures_to_verify = vec![];

    validate_block_transactions(
        &utxo_set,
        &dr_pool,
        &b,
        &mut signatures_to_verify,
        &rep_eng,
        genesis_block_hash,
        EpochConstants::default(),
        block_number,
        ONE_WIT,
    )?;
    verify_signatures_test(signatures_to_verify)?;

    Ok(())
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
    let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();

    let x = test_block(|b| {
        assert_ne!(unknown_hash, b.block_header.beacon.hash_prev_block);
        b.block_header.beacon.hash_prev_block = unknown_hash;

        // Re-create a valid VRF proof
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey {
            bytes: Protected::from(vec![0xcd; 32]),
        };

        b.block_header.proof =
            BlockEligibilityClaim::create(vrf, &secret_key, b.block_header.beacon).unwrap();

        true
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::PreviousHashMismatch {
            block_hash: unknown_hash,
            our_hash: last_block_hash,
        },
    );
}

#[test]
fn block_hash_prev_block_genesis_hash() {
    // This is a regression test for issue #797
    // Make sure that blocks with prev_block_hash equal to genesis_hash are only accepted when
    // checkpoint_beacon.hash_prev_block is equal to unknown_hash
    let genesis_hash = Hash::default();
    let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();

    let x = test_block(|b| {
        assert_ne!(genesis_hash, b.block_header.beacon.hash_prev_block);
        b.block_header.beacon.hash_prev_block = genesis_hash;

        // Re-create a valid VRF proof
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey {
            bytes: Protected::from(vec![0xcd; 32]),
        };

        b.block_header.proof =
            BlockEligibilityClaim::create(vrf, &secret_key, b.block_header.beacon).unwrap();

        true
    });
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::PreviousHashMismatch {
            block_hash: genesis_hash,
            our_hash: last_block_hash,
        },
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
    let mut signatures_to_verify = vec![];
    let dr_pool = DataRequestPool::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();

    // Create a reputation engine with 512 identities
    let mut rep_eng = ReputationEngine::new(100);
    rep_eng
        .ars_mut()
        .push_activity((0..512).map(|x| PublicKeyHash::from_hex(&format!("{:040}", x)).unwrap()));
    let mut utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    // Insert output to utxo
    let output1 = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000_000,
    };
    //let tx_output1 = VTTransactionBody::new(vec![], vec![output1.clone()]);
    //let output1_pointer = OutputPointer { transaction_id: tx_output1.hash(), output_index: 0 };
    let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
    utxo_set.insert(output1_pointer, output1, block_number);

    let secret_key = SecretKey {
        bytes: Protected::from(vec![0xcd; 32]),
    };
    let current_epoch = 1000;
    let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let block_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let my_pkh = PublicKeyHash::default();
    let mining_bf = 8;
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let genesis_block_hash = GENESIS_BLOCK_HASH.parse().unwrap();

    let mut txns = BlockTransactions::default();
    txns.mint = MintTransaction::new(
        current_epoch,
        ValueTransferOutput {
            time_lock: 0,
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
        let x = || -> Result<_, failure::Error> {
            validate_candidate(
                &b,
                current_epoch,
                &mut signatures_to_verify,
                u32::try_from(rep_eng.ars().active_identities_number())?,
                mining_bf,
            )?;
            verify_signatures_test(signatures_to_verify)?;
            let mut signatures_to_verify = vec![];

            validate_block(
                &b,
                current_epoch,
                chain_beacon,
                &mut signatures_to_verify,
                &rep_eng,
                mining_bf,
                bootstrap_hash,
                genesis_block_hash,
            )?;
            verify_signatures_test(signatures_to_verify)?;
            let mut signatures_to_verify = vec![];

            validate_block_transactions(
                &utxo_set,
                &dr_pool,
                &b,
                &mut signatures_to_verify,
                &rep_eng,
                genesis_block_hash,
                EpochConstants::default(),
                block_number,
                ONE_WIT,
            )?;
            verify_signatures_test(signatures_to_verify)?;

            Ok(())
        };

        x()
    };

    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::BlockEligibilityDoesNotMeetTarget {
            vrf_hash: "40167423312aad76b13613d822d8fc677b8db84667202c33fbbaeb3008906bdc"
                .parse()
                .unwrap(),
            target_hash: Hash::with_first_u32(0x03ff_ffff),
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
                time_lock: 0,
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
            time_lock: 0,
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
            time_lock: 0,
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
                time_lock: 0,
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

#[test]
fn block_duplicated_commits() {
    let mut dr_pool = DataRequestPool::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();

    let secret_key = SecretKey {
        bytes: Protected::from(vec![0xcd; 32]),
    };
    let current_epoch = 1000;
    let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();
    let block_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    // Add commits
    let commit_beacon = block_beacon;

    let dro = DataRequestOutput {
        witness_reward: 1000 / 2,
        commit_fee: 50,
        witnesses: 2,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    let dr_epoch = 0;
    dr_pool
        .process_data_request(
            &dr_transaction,
            dr_epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof =
        DataRequestEligibilityClaim::create(vrf, &secret_key, commit_beacon, dr_hash).unwrap();
    // Sign commitment
    let cs = sign_t(&cb);
    let c_tx = CommitTransaction::new(cb.clone(), vec![cs]);

    let mut cb2 = CommitTransactionBody::default();
    cb2.dr_pointer = cb.dr_pointer;
    cb2.proof = cb.proof;
    cb2.commitment = Hash::SHA256([1; 32]);
    let cs2 = sign_t(&cb2);
    let c2_tx = CommitTransaction::new(cb2, vec![cs2]);

    assert_ne!(c_tx.hash(), c2_tx.hash());

    let x = test_block_with_drpool(
        |b| {
            // We include two commits with same pkh and dr_pointer
            b.txns.commit_txns.push(c_tx.clone());
            b.txns.commit_txns.push(c2_tx.clone());

            b.txns.mint = MintTransaction::new(
                b.txns.mint.epoch,
                ValueTransferOutput {
                    time_lock: 0,
                    value: b.txns.mint.output.value + 100, // reveal_fee is 50*2
                    ..b.txns.mint.output
                },
            );

            build_merkle_tree(&mut b.block_header, &b.txns);

            true
        },
        dr_pool,
    );

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DuplicatedCommit {
            pkh: c_tx.body.proof.proof.pkh(),
            dr_pointer: dr_hash,
        },
    );
}

#[test]
fn block_duplicated_reveals() {
    let mut dr_pool = DataRequestPool::default();
    let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();

    // Add commits
    let dro = DataRequestOutput {
        witness_reward: 1100 / 2,
        witnesses: 2,
        reveal_fee: 50,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_t(&dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    let dr_epoch = 0;
    dr_pool
        .process_data_request(
            &dr_transaction,
            dr_epoch,
            EpochConstants::default(),
            &Hash::default(),
        )
        .unwrap();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_t(&RevealTransactionBody::default()).public_key;
    let public_key2 = sign_t2(&RevealTransactionBody::default()).public_key;

    let dr_pointer = dr_hash;

    // Create Reveal and Commit
    // Reveal = empty array
    let reveal_value = vec![0x00];
    let reveal_body =
        RevealTransactionBody::new(dr_pointer, reveal_value.clone(), public_key.pkh());
    let reveal_signature = sign_t(&reveal_body);
    let commitment = reveal_signature.signature.hash();

    let commit_transaction = CommitTransaction::new(
        CommitTransactionBody::without_collateral(
            dr_pointer,
            commitment,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key,
        }],
    );
    let reveal_transaction = RevealTransaction::new(reveal_body, vec![reveal_signature]);

    let reveal_body2 = RevealTransactionBody::new(dr_pointer, reveal_value, public_key2.pkh());
    let reveal_signature2 = sign_t2(&reveal_body2);
    let commitment2 = reveal_signature2.signature.hash();

    let commit_transaction2 = CommitTransaction::new(
        CommitTransactionBody::without_collateral(
            dr_pointer,
            commitment2,
            DataRequestEligibilityClaim::default(),
        ),
        vec![KeyedSignature {
            signature: Signature::default(),
            public_key: public_key2,
        }],
    );

    // Include CommitTransaction in DataRequestPool
    dr_pool
        .process_commit(&commit_transaction, &last_block_hash)
        .unwrap();
    dr_pool
        .process_commit(&commit_transaction2, &last_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();

    let x = test_block_with_drpool(
        |b| {
            // We include two reveals with same pkh and dr_pointer
            b.txns.reveal_txns.push(reveal_transaction.clone());
            b.txns.reveal_txns.push(reveal_transaction.clone());

            b.txns.mint = MintTransaction::new(
                b.txns.mint.epoch,
                ValueTransferOutput {
                    time_lock: 0,
                    value: b.txns.mint.output.value + 100, // reveal_fee is 50*2
                    ..b.txns.mint.output
                },
            );

            build_merkle_tree(&mut b.block_header, &b.txns);

            true
        },
        dr_pool,
    );

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DuplicatedReveal {
            pkh: reveal_transaction.body.pkh,
            dr_pointer,
        },
    );
}

#[test]
fn block_duplicated_tallies() {
    let reveal_value = vec![0x00];
    let (dr_pool, dr_pointer, pkh, pkh2) = dr_pool_with_dr_in_tally_stage_2_reveals(reveal_value);

    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh,
        value: 500,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkh2,
        value: 500,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value.clone(),
        vec![vt0.clone(), vt1.clone()],
        vec![],
    );
    let tally_transaction2 = TallyTransaction::new(dr_pointer, tally_value, vec![vt1, vt0], vec![]);

    assert_ne!(tally_transaction.hash(), tally_transaction2.hash());

    let x = test_block_with_drpool(
        |b| {
            // We include two tallies with same dr_pointer
            b.txns.tally_txns.push(tally_transaction.clone());
            b.txns.tally_txns.push(tally_transaction2.clone());

            b.txns.mint = MintTransaction::new(
                b.txns.mint.epoch,
                ValueTransferOutput {
                    time_lock: 0,
                    value: b.txns.mint.output.value + 100, // tally_fee is 100
                    ..b.txns.mint.output
                },
            );

            build_merkle_tree(&mut b.block_header, &b.txns);

            true
        },
        dr_pool,
    );

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DuplicatedTally { dr_pointer },
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

        true
    });
    assert_eq!(
        x.unwrap_err()
            .downcast::<BlockError>()
            .map(|mut x| {
                // Erase block hash as it is not deterministic
                if let BlockError::VerifySignatureFail { ref mut hash } = x {
                    *hash = Default::default()
                }
                x
            })
            .unwrap(),
        BlockError::NotValidMerkleTree,
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
    let block_number = 0;
    // Insert output to utxo
    let output1 = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH.parse().unwrap(),
        value: 1_000_000,
    };
    //let tx_output1 = VTTransactionBody::new(vec![], vec![output1.clone()]);
    //let output1_pointer = OutputPointer { transaction_id: tx_output1.hash(), output_index: 0 };
    let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
    utxo_set.insert(output1_pointer, output1, block_number);

    let secret_key = SecretKey {
        bytes: Protected::from(vec![0xcd; 32]),
    };
    let mut current_epoch = 1000;
    let mut last_block_hash = LAST_BLOCK_HASH.parse().unwrap();
    let my_pkh = PublicKeyHash::default();

    for (mut txns, fees) in txns {
        // Rebuild mint
        txns.mint = MintTransaction::new(
            current_epoch,
            ValueTransferOutput {
                time_lock: 0,
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

        let mining_bf = 1;
        let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
        let genesis_block_hash = GENESIS_BLOCK_HASH.parse().unwrap();
        // First, validate candidate block (can return false positives)
        let mut signatures_to_verify = vec![];
        validate_candidate(
            &b,
            current_epoch,
            &mut signatures_to_verify,
            u32::try_from(rep_eng.ars().active_identities_number())?,
            mining_bf,
        )?;
        verify_signatures_test(signatures_to_verify)?;
        let mut signatures_to_verify = vec![];

        // Validate block VRF
        validate_block(
            &b,
            current_epoch,
            chain_beacon,
            &mut signatures_to_verify,
            &rep_eng,
            mining_bf,
            bootstrap_hash,
            genesis_block_hash,
        )?;
        verify_signatures_test(signatures_to_verify)?;
        let mut signatures_to_verify = vec![];

        // Do the expensive validation
        validate_block_transactions(
            &utxo_set,
            &dr_pool,
            &b,
            &mut signatures_to_verify,
            &rep_eng,
            genesis_block_hash,
            EpochConstants::default(),
            block_number,
            ONE_WIT,
        )?;
        verify_signatures_test(signatures_to_verify)?;

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
    x.unwrap();
}

#[test]
fn block_add_vtt_no_inputs() {
    let vt_tx_hash;
    let t0 = {
        // (actually the fee is -1)
        let extra_fee = 0;
        let vto0 = ValueTransferOutput {
            time_lock: 0,
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
            time_lock: 0,
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
    x.unwrap();
}

#[test]
fn block_add_2_vtt_same_input() {
    let t0 = {
        let vto0 = ValueTransferOutput {
            time_lock: 0,
            pkh: Default::default(),
            value: 1,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0]);
        let vts = sign_t(&vt_body);
        let vt_tx1 = VTTransaction::new(vt_body, vec![vts]);

        let vto0 = ValueTransferOutput {
            time_lock: 0,
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
            time_lock: 0,
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
            time_lock: 0,
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
            witness_reward: 750 / 2,
            witnesses: 2,
            min_consensus_percentage: 51,
            collateral: ONE_WIT,
            data_request,
            ..DataRequestOutput::default()
        };

        let vto0 = ValueTransferOutput {
            time_lock: 0,
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
    x.unwrap();
}

#[test]
fn block_add_2_drt_same_input() {
    let t0 = {
        let data_request = example_data_request();
        let dr_output = DataRequestOutput {
            witness_reward: 750 / 2,
            witnesses: 2,
            min_consensus_percentage: 51,
            collateral: ONE_WIT,
            data_request,
            ..DataRequestOutput::default()
        };

        let vto0 = ValueTransferOutput {
            time_lock: 0,
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
            witness_reward: 750 / 2,
            witnesses: 2,
            collateral: ONE_WIT,
            data_request,
            ..DataRequestOutput::default()
        };

        let vto0 = ValueTransferOutput {
            time_lock: 0,
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
            witness_reward: 750 / 2,
            witnesses: 2,
            min_consensus_percentage: 51,
            data_request,
            collateral: ONE_WIT,
            ..DataRequestOutput::default()
        };

        let vto0 = ValueTransferOutput {
            time_lock: 0,
            pkh: Default::default(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let dr_tx_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0], dr_output);
        let drs = sign_t(&dr_tx_body);
        let dr_tx = DRTransaction::new(dr_tx_body, vec![drs]);

        let vto0 = ValueTransferOutput {
            time_lock: 0,
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

#[test]
fn genesis_block_empty() {
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let b = Block::genesis(bootstrap_hash, vec![]);

    validate_genesis_block(&b, b.hash()).unwrap();
}

#[test]
fn genesis_block_bootstrap_hash_mismatch() {
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let expected_genesis_hash = Hash::default();
    let b = Block::genesis(bootstrap_hash, vec![]);

    let x = validate_genesis_block(&b, expected_genesis_hash);
    assert_eq!(
        x.unwrap_err(),
        BlockError::GenesisBlockHashMismatch {
            block_hash: b.hash(),
            expected_hash: expected_genesis_hash,
        }
    );
}

#[test]
fn genesis_block_add_vtt() {
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let mut b = Block::genesis(bootstrap_hash, vec![]);
    // Add an extra VTT without updating the merkle root, not changing the block hash
    b.txns
        .value_transfer_txns
        .push(VTTransaction::genesis(vec![ValueTransferOutput {
            pkh: MY_PKH.parse().unwrap(),
            value: 1000,
            time_lock: 0,
        }]));

    let x = validate_genesis_block(&b, b.hash());
    // Compare only enum variant
    assert_eq!(
        std::mem::discriminant(&x.unwrap_err()),
        std::mem::discriminant(&BlockError::GenesisBlockMismatch {
            block: "".to_string(),
            expected: "".to_string(),
        })
    );
}

#[test]
fn genesis_block_add_signature() {
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let mut b = Block::genesis(bootstrap_hash, vec![]);
    // Add an extra signature, not changing the block hash
    b.block_sig = sign_t(&b);

    let x = validate_genesis_block(&b, b.hash());
    // Compare only enum variant
    assert_eq!(
        std::mem::discriminant(&x.unwrap_err()),
        std::mem::discriminant(&BlockError::GenesisBlockMismatch {
            block: "".to_string(),
            expected: "".to_string(),
        })
    );
}
#[test]
fn genesis_block_after_not_bootstrap_hash() {
    // Try to consolidate genesis block when chain beacon hash_prev_block
    // is different from bootstrap hash
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let b = Block::genesis(bootstrap_hash, vec![]);

    let rep_eng = ReputationEngine::new(100);

    let current_epoch = 0;
    // If this was bootstrap hash, the validation would pass:
    let last_block_hash = b.hash();
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };

    let mining_bf = 1;
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let genesis_block_hash = b.hash();
    let mut signatures_to_verify = vec![];

    // Validate block
    let x = validate_block(
        &b,
        current_epoch,
        chain_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        mining_bf,
        bootstrap_hash,
        genesis_block_hash,
    );
    assert_eq!(signatures_to_verify, vec![]);

    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::PreviousHashMismatch {
            block_hash: b.block_header.beacon.hash_prev_block,
            our_hash: last_block_hash,
        }
    );
}

#[test]
fn genesis_block_value_overflow() {
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let outputs = vec![ValueTransferOutput {
        pkh: MY_PKH.parse().unwrap(),
        value: u64::max_value(),
        time_lock: 0,
    }];
    let b = Block::genesis(
        bootstrap_hash,
        vec![
            VTTransaction::genesis(outputs.clone()),
            VTTransaction::genesis(outputs),
        ],
    );

    let dr_pool = DataRequestPool::default();
    let rep_eng = ReputationEngine::new(100);
    let utxo_set = UnspentOutputsPool::default();

    let current_epoch = 0;
    let block_number = 0;
    let last_block_hash = bootstrap_hash;
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };

    let mining_bf = 1;
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let genesis_block_hash = b.hash();
    let mut signatures_to_verify = vec![];

    // Validate block
    validate_block(
        &b,
        current_epoch,
        chain_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        mining_bf,
        bootstrap_hash,
        genesis_block_hash,
    )
    .unwrap();
    assert_eq!(signatures_to_verify, vec![]);
    let mut signatures_to_verify = vec![];

    // Do the expensive validation
    let x = validate_block_transactions(
        &utxo_set,
        &dr_pool,
        &b,
        &mut signatures_to_verify,
        &rep_eng,
        genesis_block_hash,
        EpochConstants::default(),
        block_number,
        ONE_WIT,
    );
    assert_eq!(signatures_to_verify, vec![]);
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::GenesisValueOverflow {
            max_total_value: u64::max_value() - total_block_reward(),
        },
    );
}

#[test]
fn genesis_block_full_validate() {
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let b = Block::genesis(bootstrap_hash, vec![]);

    let dr_pool = DataRequestPool::default();
    let rep_eng = ReputationEngine::new(100);
    let utxo_set = UnspentOutputsPool::default();

    let current_epoch = 0;
    let block_number = 0;
    let last_block_hash = bootstrap_hash;
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };

    let mining_bf = 1;
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let genesis_block_hash = b.hash();
    let mut signatures_to_verify = vec![];

    // Validate block
    validate_block(
        &b,
        current_epoch,
        chain_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        mining_bf,
        bootstrap_hash,
        genesis_block_hash,
    )
    .unwrap();
    assert_eq!(signatures_to_verify, vec![]);
    let mut signatures_to_verify = vec![];

    // Do the expensive validation
    validate_block_transactions(
        &utxo_set,
        &dr_pool,
        &b,
        &mut signatures_to_verify,
        &rep_eng,
        genesis_block_hash,
        EpochConstants::default(),
        block_number,
        ONE_WIT,
    )
    .unwrap();
    assert_eq!(signatures_to_verify, vec![]);
}

#[test]
fn validate_block_transactions_uses_block_number_in_utxo_diff() {
    // Check that the UTXO diff returned by validate_block_transactions respects the block number
    let block_number = 1234;

    let utxo_diff = {
        let dr_pool = DataRequestPool::default();
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let rep_eng = ReputationEngine::new(100);
        let utxo_set = UnspentOutputsPool::default();

        let secret_key = SecretKey {
            bytes: Protected::from(vec![0xcd; 32]),
        };
        let current_epoch = 1000;
        let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();
        let block_beacon = CheckpointBeacon {
            checkpoint: current_epoch,
            hash_prev_block: last_block_hash,
        };
        let my_pkh = PublicKeyHash::default();
        let genesis_block_hash = GENESIS_BLOCK_HASH.parse().unwrap();

        let mut txns = BlockTransactions::default();
        txns.mint = MintTransaction::new(
            current_epoch,
            ValueTransferOutput {
                time_lock: 0,
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
        let mut signatures_to_verify = vec![];

        validate_block_transactions(
            &utxo_set,
            &dr_pool,
            &b,
            &mut signatures_to_verify,
            &rep_eng,
            genesis_block_hash,
            EpochConstants::default(),
            block_number,
            ONE_WIT,
        )
        .unwrap()
    };

    // Apply the UTXO diff to an empty UTXO set
    let mut utxo_set = UnspentOutputsPool::default();
    utxo_diff.apply(&mut utxo_set);

    // This will only check one transaction: the mint transaction
    // But in the UTXO set there are no transactions, only outputs, so all the
    // other transactions should follow the same behaviour
    assert_eq!(utxo_set.iter().count(), 1);
    for (output_pointer, _vto) in utxo_set.iter() {
        assert_eq!(
            utxo_set.included_in_block_number(output_pointer),
            Some(block_number)
        );
    }
}
