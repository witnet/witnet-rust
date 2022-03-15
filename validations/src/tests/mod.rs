use std::{
    collections::HashSet,
    convert::TryFrom,
    sync::atomic::{AtomicU32, Ordering},
};

use itertools::Itertools;

use witnet_crypto::{
    key::CryptoEngine,
    secp256k1::{PublicKey as Secp256k1_PublicKey, Secp256k1, SecretKey as Secp256k1_SecretKey},
    signature::sign,
};
use witnet_data_structures::{
    chain::*,
    data_request::{
        calculate_tally_change, calculate_witness_reward, create_tally, DataRequestPool,
    },
    error::{BlockError, DataRequestError, Secp256k1ConversionError, TransactionError},
    mainnet_validations::{
        all_wips_active, current_active_wips, ActiveWips, TapiEngine, FIRST_HARD_FORK,
    },
    radon_error::RadonError,
    radon_report::{RadonReport, ReportContext, TypeLike},
    transaction::*,
    transaction_factory::transaction_outputs_sum,
    utxo_pool::{UnspentOutputsPool, UtxoDiff},
    vrf::{BlockEligibilityClaim, DataRequestEligibilityClaim, VrfCtx},
};
use witnet_protected::Protected;
use witnet_rad::{
    conditions::*,
    error::RadError,
    filters::RadonFilters,
    reducers::RadonReducers,
    types::{bytes::RadonBytes, integer::RadonInteger, RadonTypes},
};

use crate::validations::*;

mod compare_block_candidates;
mod randpoe;
mod reppoe;
mod tally_precondition;

static ONE_WIT: u64 = 1_000_000_000;
const MAX_VT_WEIGHT: u32 = 20_000;
const MAX_DR_WEIGHT: u32 = 80_000;
const INITIAL_BLOCK_REWARD: u64 = 250 * 1_000_000_000;
const HALVING_PERIOD: u32 = 3_500_000;
const LAST_EPOCH_WITH_WIP_ACTIVATED: u32 = 683_541;

// Block epoch used in tally tests
const E: Epoch = LAST_EPOCH_WITH_WIP_ACTIVATED;

// This should only be used in tests
fn active_wips_from_mainnet(block_epoch: Epoch) -> ActiveWips {
    let mut tapi_engine = TapiEngine::default();
    tapi_engine.initialize_wip_information(Environment::Mainnet);

    ActiveWips {
        active_wips: tapi_engine.wip_activation,
        block_epoch,
    }
}

fn verify_signatures_test(
    signatures_to_verify: Vec<SignaturesToVerify>,
) -> Result<(), failure::Error> {
    let secp = &CryptoEngine::new();
    let vrf = &mut VrfCtx::secp256k1().unwrap();

    verify_signatures(signatures_to_verify, vrf, secp).map(|_| ())
}

fn sign_tx<H: Hashable>(mk: [u8; 32], tx: &H) -> KeyedSignature {
    let Hash::SHA256(data) = tx.hash();

    let secp = &Secp256k1::new();
    let secret_key = Secp256k1_SecretKey::from_slice(&mk).expect("32 bytes, within curve order");
    let public_key = Secp256k1_PublicKey::from_secret_key(secp, &secret_key);
    let public_key = PublicKey::from(public_key);

    let signature = sign(secp, secret_key, &data).unwrap();

    KeyedSignature {
        signature: Signature::from(signature),
        public_key,
    }
}
static PRIV_KEY_1: [u8; 32] = [0xcd; 32];
static PRIV_KEY_2: [u8; 32] = [0x43; 32];
static PRIV_KEY_3: [u8; 32] = [0x69; 32];
static MY_PKH_1: &str = "wit18cfejmk3305y9kw5xqa59rwnpjzahr57us48vm";
static MY_PKH_2: &str = "wit1z8mxkml4a50dyysqczsp7gj5pnvz3jsldras8t";
static MY_PKH_3: &str = "wit164gu2l8p7suvc7zq5xvc27h63td75g6uspwpn5";

#[test]
fn test_sign_tx() {
    let tx = &RevealTransaction::default();

    let ks = sign_tx(PRIV_KEY_1, tx);
    assert_eq!(ks.public_key.pkh(), MY_PKH_1.parse().unwrap());

    let ks = sign_tx(PRIV_KEY_2, tx);
    assert_eq!(ks.public_key.pkh(), MY_PKH_2.parse().unwrap());

    let ks = sign_tx(PRIV_KEY_3, tx);
    assert_eq!(ks.public_key.pkh(), MY_PKH_3.parse().unwrap());
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
            vec![o],
        ))
    }));

    let all_utxos = all_utxos.into().unwrap_or_default();
    let block_number = 0;

    generate_unspent_outputs_pool(&all_utxos, &txns, block_number)
}

fn update_utxo_inputs(utxo: &mut UnspentOutputsPool, inputs: &[Input]) {
    for input in inputs {
        // Obtain the OutputPointer of each input and remove it from the utxo_set
        let output_pointer = input.output_pointer();

        // This does check for missing inputs, so ignore "fake inputs" with hash 000000...
        if output_pointer.transaction_id != Hash::default() {
            utxo.remove(output_pointer);
        }
    }
}

fn update_utxo_outputs(
    utxo: &mut UnspentOutputsPool,
    outputs: &[ValueTransferOutput],
    txn_hash: Hash,
    block_number: u32,
) {
    for (index, output) in outputs.iter().enumerate() {
        // Add the new outputs to the utxo_set
        let output_pointer = OutputPointer {
            transaction_id: txn_hash,
            output_index: u32::try_from(index).unwrap(),
        };

        utxo.insert(output_pointer, output.clone(), block_number);
    }
}

/// Method to update the unspent outputs pool
pub fn generate_unspent_outputs_pool(
    unspent_outputs_pool: &UnspentOutputsPool,
    transactions: &[Transaction],
    block_number: u32,
) -> UnspentOutputsPool {
    // Create a copy of the state "unspent_outputs_pool"
    let mut unspent_outputs = unspent_outputs_pool.clone();

    for transaction in transactions {
        let txn_hash = transaction.hash();
        match transaction {
            Transaction::ValueTransfer(vt_transaction) => {
                update_utxo_inputs(&mut unspent_outputs, &vt_transaction.body.inputs);
                update_utxo_outputs(
                    &mut unspent_outputs,
                    &vt_transaction.body.outputs,
                    txn_hash,
                    block_number,
                );
            }
            Transaction::DataRequest(dr_transaction) => {
                update_utxo_inputs(&mut unspent_outputs, &dr_transaction.body.inputs);
                update_utxo_outputs(
                    &mut unspent_outputs,
                    &dr_transaction.body.outputs,
                    txn_hash,
                    block_number,
                );
            }
            Transaction::Tally(tally_transaction) => {
                update_utxo_outputs(
                    &mut unspent_outputs,
                    &tally_transaction.outputs,
                    txn_hash,
                    block_number,
                );
            }
            Transaction::Mint(mint_transaction) => {
                update_utxo_outputs(
                    &mut unspent_outputs,
                    &mint_transaction.outputs,
                    txn_hash,
                    block_number,
                );
            }
            _ => {}
        }
    }

    unspent_outputs
}

// Validate transactions in block
#[test]
fn mint_mismatched_reward() {
    let epoch = 0;
    let total_fees = 100;
    let reward = block_reward(epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD);
    // Build mint without the block reward
    let output = ValueTransferOutput {
        pkh: Default::default(),
        value: 100,
        time_lock: 0,
    };
    let mint_tx = MintTransaction::new(epoch, vec![output]);
    let x = validate_mint_transaction(
        &mint_tx,
        total_fees,
        epoch,
        INITIAL_BLOCK_REWARD,
        HALVING_PERIOD,
    );
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
    let reward = block_reward(epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD);
    let total_fees = 100;
    let output = ValueTransferOutput {
        pkh: Default::default(),
        value: reward + total_fees,
        time_lock: 0,
    };
    // Build a mint for the next epoch
    let mint_tx = MintTransaction::new(epoch + 1, vec![output]);
    let x = validate_mint_transaction(
        &mint_tx,
        total_fees,
        epoch,
        INITIAL_BLOCK_REWARD,
        HALVING_PERIOD,
    );
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
fn mint_multiple_split() {
    let epoch = 0;
    let reward = block_reward(epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD);
    let total_fees = 100;
    let output1 = ValueTransferOutput {
        pkh: Default::default(),
        value: reward + total_fees,
        time_lock: 0,
    };
    let output2 = ValueTransferOutput {
        pkh: Default::default(),
        value: 0,
        time_lock: 0,
    };
    let output3 = ValueTransferOutput {
        pkh: Default::default(),
        value: 0,
        time_lock: 0,
    };
    let mint_tx = MintTransaction::new(epoch, vec![output1, output2, output3]);
    let x = validate_mint_transaction(
        &mint_tx,
        total_fees,
        epoch,
        INITIAL_BLOCK_REWARD,
        HALVING_PERIOD,
    );
    // Error: Mint outputs smaller than collateral minimum
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::TooSplitMint
    );
}

#[test]
fn mint_split_valid() {
    let epoch = 0;
    let reward = block_reward(epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD);
    let total_fees = 100;
    let output1 = ValueTransferOutput {
        pkh: Default::default(),
        value: total_fees,
        time_lock: 0,
    };
    let output2 = ValueTransferOutput {
        pkh: Default::default(),
        value: reward,
        time_lock: 0,
    };
    let mint_tx = MintTransaction::new(epoch, vec![output1, output2]);
    let x = validate_mint_transaction(
        &mint_tx,
        total_fees,
        epoch,
        INITIAL_BLOCK_REWARD,
        HALVING_PERIOD,
    );
    x.unwrap();
}

#[test]
fn mint_valid() {
    let epoch = 0;
    let reward = block_reward(epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD);
    let total_fees = 100;
    let output = ValueTransferOutput {
        pkh: Default::default(),
        value: total_fees + reward,
        time_lock: 0,
    };
    let mint_tx = MintTransaction::new(epoch, vec![output]);
    let x = validate_mint_transaction(
        &mint_tx,
        total_fees,
        epoch,
        INITIAL_BLOCK_REWARD,
        HALVING_PERIOD,
    );
    x.unwrap();
}

#[test]
fn vtt_no_inputs_no_outputs() {
    let mut signatures_to_verify = vec![];
    let utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);

    let vt_body = VTTransactionBody::new(vec![], vec![]);
    let vt_tx = VTTransaction::new(vt_body, vec![]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
    let utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);

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
        MAX_VT_WEIGHT,
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
    let utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);

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
        MAX_VT_WEIGHT,
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
    let utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);

    // No inputs but 1 signature
    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![], vec![vto0]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
    let utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
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
        MAX_VT_WEIGHT,
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
    let ks = sign_tx(PRIV_KEY_1, &hashable);
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
                expected_pkh: MY_PKH_1.parse().unwrap(),
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
            msg: "secp: signature failed verification".to_string(),
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
            // A "secp: signature failed verification" msg would also be correct here
            msg: TransactionError::PublicKeyHashMismatch {
                expected_pkh: MY_PKH_1.parse().unwrap(),
                signature_pkh,
            }
            .to_string(),
        }
    );

    // Sign transaction with a different public key
    let ks_different_pk = sign_tx(PRIV_KEY_2, &hashable);
    let signature_pkh = ks_different_pk.public_key.pkh();
    let x = f(hashable, ks_different_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash,
            msg: TransactionError::PublicKeyHashMismatch {
                expected_pkh: MY_PKH_1.parse().unwrap(),
                signature_pkh,
            }
            .to_string(),
        }
    );
}

#[test]
fn vtt_one_input_signatures() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

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
            MAX_VT_WEIGHT,
        )?;
        verify_signatures_test(signatures_to_verify)?;

        Ok(())
    });
}

#[test]
fn vtt_input_not_in_utxo() {
    let mut signatures_to_verify = vec![];
    let utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
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
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    let zero_output = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 0,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![zero_output]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 2,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1,
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 2,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0, vto1]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 21,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 13,
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 7,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0, vto1]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
    )
    .map(|(_, _, fee)| fee);
    assert_eq!(x.unwrap(), 21 - 13 - 7,);
}

#[test]
fn vtt_two_inputs_one_signature() {
    let mut signatures_to_verify = vec![];
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 21,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 13,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti0 = Input::new(utxo_set.iter().next().unwrap().0);
    let vti1 = Input::new(utxo_set.iter().nth(1).unwrap().0);

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 10,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 21,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 13,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti0 = Input::new(utxo_set.iter().next().unwrap().0);
    let vti1 = Input::new(utxo_set.iter().nth(1).unwrap().0);

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 10,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vts2 = sign_tx(PRIV_KEY_2, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts, vts2]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::VerifyTransactionSignatureFail {
            hash: vt_tx.hash(),
            msg: TransactionError::PublicKeyHashMismatch {
                expected_pkh: MY_PKH_1.parse().unwrap(),
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 21,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 13,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti0 = Input::new(utxo_set.iter().next().unwrap().0);
    let vti1 = Input::new(utxo_set.iter().nth(1).unwrap().0);

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 10,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts.clone(), vts.clone(), vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 21,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 13,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti0 = Input::new(utxo_set.iter().next().unwrap().0);
    let vti1 = Input::new(utxo_set.iter().nth(1).unwrap().0);

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 10,
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 20,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0, vto1]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts.clone(), vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
    )
    .map(|(_, _, fee)| fee);
    assert_eq!(x.unwrap(), 21 + 13 - 10 - 20,);
}

#[test]
fn vtt_input_value_overflow() {
    let mut signatures_to_verify = vec![];
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: u64::max_value(),
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1_000,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti0 = Input::new(utxo_set.iter().next().unwrap().0);
    let vti1 = Input::new(utxo_set.iter().nth(1).unwrap().0);

    // The total output value should not overflow
    let vto0 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: u64::max_value() - 10,
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 10,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0, vto1]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts; 2]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: u64::max_value() - 1_000,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1_000,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti0 = Input::new(utxo_set.iter().next().unwrap().0);
    let vti1 = Input::new(utxo_set.iter().nth(1).unwrap().0);

    // The total output value should overflow
    let vto0 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: u64::max_value(),
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1_000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti0, vti1], vec![vto0, vto1]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts; 2]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
            pkh: MY_PKH_1.parse().unwrap(),
            value: 1000,
            time_lock,
        };
        let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
        let block_number = 0;
        let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
        let vti = Input::new(utxo_set.iter().next().unwrap().0);

        let pkh = PublicKeyHash::default();
        let vto0 = ValueTransferOutput {
            pkh,
            value: 1000,
            time_lock: 0,
        };

        let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);
        let mut signatures_to_verify = vec![];
        validate_vt_transaction(
            &vt_tx,
            &utxo_diff,
            epoch,
            epoch_constants,
            &mut signatures_to_verify,
            MAX_VT_WEIGHT,
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
fn vtt_validation_weight_limit_exceeded() {
    let mut signatures_to_verify = vec![];
    let utxo_set = build_utxo_set_with_mint(vec![], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_set, 1000);

    let vt_body =
        VTTransactionBody::new(vec![Input::default()], vec![ValueTransferOutput::default()]);
    let vt_tx = VTTransaction::new(vt_body, vec![]);
    let vt_weight = vt_tx.weight();
    assert_eq!(vt_weight, 493);

    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        493 - 1,
    );

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::ValueTransferWeightLimitExceeded {
            weight: 493,
            max_weight: 493 - 1
        }
    );
}

#[test]
fn vtt_valid() {
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    let pkh = PublicKeyHash::default();
    let vto0 = ValueTransferOutput {
        pkh,
        value: 1000,
        time_lock: 0,
    };

    let vt_body = VTTransactionBody::new(vec![vti], vec![vto0]);
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
    let vt_tx = VTTransaction::new(vt_body, vec![vts]);
    let x = validate_vt_transaction(
        &vt_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        MAX_VT_WEIGHT,
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
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
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
    let vts = sign_tx(PRIV_KEY_1, &vt_body);
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
    // Try to create a data request with no inputs
    // A data request with no inputs may cause a panic in validations because we need to get the pkh
    // of the first input, and in this case there is no first input
    // This is mitigated by checking that there is at least one input, and returning ZeroAmount
    // error if there are no inputs
    let mut signatures_to_verify = vec![];
    let utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);

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
        MAX_DR_WEIGHT,
        &current_active_wips(),
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::ZeroAmount
    );
}

#[test]
fn data_request_no_inputs_but_one_signature() {
    let mut signatures_to_verify = vec![];
    let utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);

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
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);
    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

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
        MAX_DR_WEIGHT,
        &current_active_wips(),
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

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
            MAX_DR_WEIGHT,
            &current_active_wips(),
        )?;
        verify_signatures_test(signatures_to_verify)?;

        Ok(())
    });
}

#[test]
fn data_request_input_double_spend() {
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let output = utxo_set.iter().next().unwrap().0;
    let vti = Input::new(output.clone());

    let dr_output = DataRequestOutput {
        witness_reward: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request: example_data_request(),
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti; 2], vec![], dr_output);
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs; 2]);
    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::OutputNotFound { output }
    );
}

#[test]
fn data_request_input_not_in_utxo() {
    let mut signatures_to_verify = vec![];
    let utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
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
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);
    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    let dr_output = DataRequestOutput {
        witness_reward: 500,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };

    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);
    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeFee
    );
}

#[test]
fn data_request_output_value_overflow() {
    // Try to create a value transfer output with 2 outputs with value near u64::max_value()
    // This may cause an overflow in the fee validations if implemented incorrectly
    // The current implementation handles this by rejecting data requests with more than 1 value
    // transfer output
    let mut signatures_to_verify = vec![];
    let vto_21 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: u64::max_value() - 1_000,
        time_lock: 0,
    };
    let vto_13 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1_000,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto_21, vto_13], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti0 = Input::new(utxo_set.iter().next().unwrap().0);
    let vti1 = Input::new(utxo_set.iter().nth(1).unwrap().0);

    let vto0 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: u64::max_value() - 1_000,
        time_lock: 0,
    };
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
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
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs; 2]);
    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::WrongNumberOutputs {
            outputs: 2,
            expected_outputs: 1,
        }
    );
}

// Helper function which creates a data request with a valid input with value 1000
// and returns the validation error
fn test_drtx(dr_output: DataRequestOutput) -> Result<(), failure::Error> {
    let mut signatures_to_verify = vec![];
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        u32::max_value(),
        &all_wips_active(),
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
            url: "https://blockchain.info/q/latesthash".to_string(),
            script: vec![128],
            body: vec![],
            headers: vec![],
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

// Example data request used in tests. It consists of just empty arrays.
// It is used for tests that works before WIP-0019
fn example_data_request_before_wip19() -> RADRequest {
    RADRequest {
        time_lock: 0,
        retrieve: vec![RADRetrieve {
            kind: RADType::Unknown,
            url: "https://blockchain.info/q/latesthash".to_string(),
            script: vec![128],
            body: vec![],
            headers: vec![],
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

fn example_data_request_average_mean_reducer() -> RADRequest {
    RADRequest {
        time_lock: 0,
        retrieve: vec![RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://blockchain.info/q/latesthash".to_string(),
            script: vec![128],
            body: vec![],
            headers: vec![],
        }],
        aggregate: RADAggregate {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        },
        tally: RADTally {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
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
            body: vec![],
            headers: vec![],
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

fn example_data_request_rng() -> RADRequest {
    RADRequest {
        time_lock: 0,
        retrieve: vec![RADRetrieve {
            kind: RADType::Rng,
            url: "".to_string(),
            script: vec![],
            body: vec![],
            headers: vec![],
        }],
        aggregate: RADAggregate {
            filters: vec![],
            reducer: RadonReducers::Mode as u32,
        },
        tally: RADTally {
            filters: vec![],
            reducer: RadonReducers::HashConcatenate as u32,
        },
    }
}

fn example_data_request_http_post() -> RADRequest {
    RADRequest {
        time_lock: 0,
        retrieve: vec![RADRetrieve {
            kind: RADType::HttpPost,
            url: "https://blockchain.info/q/latesthash".to_string(),
            script: vec![128],
            body: vec![],
            headers: vec![],
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

fn example_data_request_output(witnesses: u16, witness_reward: u64, fee: u64) -> DataRequestOutput {
    DataRequestOutput {
        witnesses,
        commit_and_reveal_fee: fee,
        witness_reward,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
    }
}

fn example_data_request_output_rng(
    witnesses: u16,
    witness_reward: u64,
    fee: u64,
) -> DataRequestOutput {
    DataRequestOutput {
        witnesses,
        commit_and_reveal_fee: fee,
        witness_reward,
        min_consensus_percentage: 51,
        data_request: example_data_request_rng(),
        collateral: ONE_WIT,
    }
}

fn example_data_request_output_average_mean_reducer(
    witnesses: u16,
    witness_reward: u64,
    fee: u64,
) -> DataRequestOutput {
    DataRequestOutput {
        witnesses,
        commit_and_reveal_fee: fee,
        witness_reward,
        min_consensus_percentage: 51,
        data_request: example_data_request_average_mean_reducer(),
        collateral: ONE_WIT,
    }
}

fn example_data_request_output_with_mode_filter(
    witnesses: u16,
    witness_reward: u64,
    fee: u64,
) -> DataRequestOutput {
    DataRequestOutput {
        witnesses,
        commit_and_reveal_fee: fee,
        witness_reward,
        min_consensus_percentage: 51,
        data_request: example_data_request_with_mode_filter(),
        collateral: ONE_WIT,
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
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NoRetrievalSources,
    );
}

#[test]
fn data_request_empty_scripts() {
    let data_request = RADRequest {
        time_lock: 0,
        retrieve: vec![RADRetrieve {
            kind: RADType::HttpGet,
            url: "".to_string(),
            script: vec![0x80],
            body: vec![],
            headers: vec![],
        }],
        aggregate: RADAggregate {
            filters: vec![],
            reducer: RadonReducers::Mode as u32,
        },
        tally: RADTally {
            filters: vec![],
            reducer: RadonReducers::Mode as u32,
        },
    };

    let x = test_rad_request(data_request);
    // The data request should be invalid since the sources are empty
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::MalformedRetrieval {
            kind: RADType::HttpGet,
            expected_fields: "kind, script, url".to_string(),
            actual_fields: "kind, script".to_string(),
        },
    );
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
        commit_and_reveal_fee: 100,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
    });
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NoReward,
    );
}

#[test]
fn data_request_http_post_before_wip_activation() {
    let data_request = example_data_request_http_post();
    let dr_output = DataRequestOutput {
        witness_reward: 1,
        commit_and_reveal_fee: 100,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
    };

    let x = {
        let mut signatures_to_verify = vec![];
        let vto = ValueTransferOutput {
            pkh: MY_PKH_1.parse().unwrap(),
            value: 1000,
            time_lock: 0,
        };
        let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
        let block_number = 0;
        let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
        let vti = Input::new(utxo_set.iter().next().unwrap().0);
        let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
        let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
        let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

        let mut active_wips = all_wips_active();
        // Disable WIP0020
        active_wips.active_wips.remove("WIP0020-0021");

        validate_dr_transaction(
            &dr_transaction,
            &utxo_diff,
            Epoch::default(),
            EpochConstants::default(),
            &mut signatures_to_verify,
            ONE_WIT,
            u32::max_value(),
            &active_wips,
        )
        .map(|_| ())
    };

    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::InvalidRadType,
    );
}

#[test]
fn data_request_http_get_with_headers_before_wip_activation() {
    let data_request = RADRequest {
        time_lock: 0,
        retrieve: vec![RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://blockchain.info/q/latesthash".to_string(),
            script: vec![128],
            body: vec![],
            headers: vec![("key".to_string(), "value".to_string())],
        }],
        aggregate: RADAggregate {
            filters: vec![],
            reducer: RadonReducers::Mode as u32,
        },
        tally: RADTally {
            filters: vec![],
            reducer: RadonReducers::Mode as u32,
        },
    };
    let dr_output = DataRequestOutput {
        witness_reward: 1,
        commit_and_reveal_fee: 100,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
    };

    let x = {
        let mut signatures_to_verify = vec![];
        let vto = ValueTransferOutput {
            pkh: MY_PKH_1.parse().unwrap(),
            value: 1000,
            time_lock: 0,
        };
        let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
        let block_number = 0;
        let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
        let vti = Input::new(utxo_set.iter().next().unwrap().0);
        let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
        let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
        let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

        let mut active_wips = all_wips_active();
        // Disable WIP0020
        active_wips.active_wips.remove("WIP0020-0021");

        validate_dr_transaction(
            &dr_transaction,
            &utxo_diff,
            Epoch::default(),
            EpochConstants::default(),
            &mut signatures_to_verify,
            ONE_WIT,
            u32::max_value(),
            &active_wips,
        )
        .map(|_| ())
    };

    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::MalformedRetrieval {
            kind: RADType::HttpGet,
            expected_fields: "kind, script, url".to_string(),
            actual_fields: "headers, kind, script, url".to_string(),
        },
    );
}

#[test]
fn data_request_parse_xml_before_wip_activation() {
    let mut data_request = example_data_request_with_mode_filter();
    // [StringParseXml]
    data_request.retrieve[0].script = vec![0x81, 0x18, 0x78];
    data_request.retrieve[0].url = "http://127.0.0.1".to_string();
    let dr_output = DataRequestOutput {
        witness_reward: 1,
        commit_and_reveal_fee: 100,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
    };

    let x = {
        let mut signatures_to_verify = vec![];
        let vto = ValueTransferOutput {
            pkh: MY_PKH_1.parse().unwrap(),
            value: 1000,
            time_lock: 0,
        };
        let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
        let block_number = 0;
        let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
        let vti = Input::new(utxo_set.iter().next().unwrap().0);
        let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
        let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
        let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

        let mut active_wips = all_wips_active();
        // Disable WIP0020
        active_wips.active_wips.remove("WIP0020-0021");

        validate_dr_transaction(
            &dr_transaction,
            &utxo_diff,
            Epoch::default(),
            EpochConstants::default(),
            &mut signatures_to_verify,
            ONE_WIT,
            u32::max_value(),
            &active_wips,
        )
        .map(|_| ())
    };

    assert_eq!(
        x.unwrap_err().downcast::<RadError>().unwrap(),
        RadError::UnknownOperator { code: 0x78 },
    );
}

#[test]
fn data_request_parse_xml_after_wip_activation() {
    let mut data_request = example_data_request_with_mode_filter();
    // [StringParseXml]
    data_request.retrieve[0].script = vec![0x81, 0x18, 0x78];
    data_request.retrieve[0].url = "http://127.0.0.1".to_string();
    let dr_output = DataRequestOutput {
        witness_reward: 1,
        commit_and_reveal_fee: 100,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
    };

    let x = {
        let mut signatures_to_verify = vec![];
        let vto = ValueTransferOutput {
            pkh: MY_PKH_1.parse().unwrap(),
            value: 1000,
            time_lock: 0,
        };
        let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
        let block_number = 0;
        let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
        let vti = Input::new(utxo_set.iter().next().unwrap().0);
        let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
        let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
        let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

        let mut active_wips = all_wips_active();
        // Enable WIP0020
        active_wips
            .active_wips
            .insert("WIP0020-0021".to_string(), 0);

        validate_dr_transaction(
            &dr_transaction,
            &utxo_diff,
            Epoch::default(),
            EpochConstants::default(),
            &mut signatures_to_verify,
            ONE_WIT,
            u32::max_value(),
            &active_wips,
        )
        .map(|_| ())
    };

    x.unwrap();
}

#[test]
fn dr_validation_weight_limit_exceeded() {
    let mut signatures_to_verify = vec![];
    let utxo_set = build_utxo_set_with_mint(vec![], None, vec![]);
    let utxo_diff = UtxoDiff::new(&utxo_set, 1000);
    let dro = example_data_request_output(2, 1, 0);

    let dr_body = DRTransactionBody::new(
        vec![Input::default()],
        vec![ValueTransferOutput::default()],
        dro.clone(),
    );
    let dr_tx = DRTransaction::new(dr_body, vec![]);
    let dr_weight = dr_tx.weight();
    assert_eq!(dr_weight, 1625);

    let x = validate_dr_transaction(
        &dr_tx,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        1625 - 1,
        &current_active_wips(),
    );

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DataRequestWeightLimitExceeded {
            weight: 1625,
            max_weight: 1625 - 1,
            dr_output: dro,
        }
    );
}

#[test]
fn data_request_value_overflow() {
    let data_request = example_data_request();
    let dro = DataRequestOutput {
        witness_reward: 1,
        commit_and_reveal_fee: 1,
        witnesses: 2,
        min_consensus_percentage: 51,
        collateral: ONE_WIT,
        data_request,
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
        commit_and_reveal_fee: u64::max_value(),
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dr_output);
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let dr_miner_fee = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let change_output = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH_1.parse().unwrap(),
        value: 200,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![change_output], dr_output);
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let dr_miner_fee = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
    )
    .map(|(_, _, fee)| fee)
    .unwrap();
    assert_eq!(dr_miner_fee, 1000 - 750 - 200);
}

#[test]
fn data_request_change_to_different_pkh() {
    // Use 1000 input to pay 750 for data request, and request 200 change to a different address
    // This should fail because the change can only be sent to the same pkh as the first input
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let change_output = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH_2.parse().unwrap(),
        value: 200,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![change_output], dr_output);
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
    );

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::PublicKeyHashMismatch {
            expected_pkh: MY_PKH_1.parse().unwrap(),
            signature_pkh: MY_PKH_2.parse().unwrap(),
        }
    );
}

#[test]
fn data_request_two_change_outputs() {
    // Use 1000 input to pay 750 for data request, and request 200 change to the same address but
    // split into two outputs
    // This should fail because the data request can only have one output
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
        time_lock: 0,
    };
    let change_output_1 = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH_1.parse().unwrap(),
        value: 150,
    };
    let change_output_2 = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH_1.parse().unwrap(),
        value: 50,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);
    let dr_tx_body =
        DRTransactionBody::new(vec![vti], vec![change_output_1, change_output_2], dr_output);
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
    );

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::WrongNumberOutputs {
            outputs: 2,
            expected_outputs: 1,
        }
    );
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
    };
    let change_output = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH_1.parse().unwrap(),
        value: 300,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![change_output], dr_output);
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
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
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1000,
    };
    let change_output = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH_1.parse().unwrap(),
        value: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 0;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![change_output], dr_output);
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let x = validate_dr_transaction(
        &dr_transaction,
        &utxo_diff,
        Epoch::default(),
        EpochConstants::default(),
        &mut signatures_to_verify,
        ONE_WIT,
        MAX_DR_WEIGHT,
        &current_active_wips(),
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
    let vrf_input = CheckpointVRF::default();
    let rep_eng = ReputationEngine::new(100);
    let utxo_set = UnspentOutputsPool::default();
    let collateral_minimum = 1;
    let collateral_age = 1;
    let block_number = 0;
    let minimum_reppoe_difficulty = 1;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);

    validate_commit_transaction(
        c_tx,
        &dr_pool,
        vrf_input,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
        &utxo_diff,
        collateral_minimum,
        collateral_age,
        block_number,
        minimum_reppoe_difficulty,
        &current_active_wips(),
    )
    .map(|_| ())
}

static DR_HASH: &str = "469dc46106ef5008cc5a6106ff9dedcf4ac19a23b1ea41807ae1fc08ab79a08e";

// Helper function to test a commit with an empty state (no utxos, no drs, etc)
fn test_commit_with_dr_and_utxo_set(
    c_tx: &CommitTransaction,
    utxo_set: &UnspentOutputsPool,
) -> Result<(), failure::Error> {
    let block_number = 100_000;
    let utxo_diff = UtxoDiff::new(utxo_set, 0);
    let collateral_minimum = 1;
    let collateral_age = 1;
    let minimum_reppoe_difficulty = 1;

    let mut dr_pool = DataRequestPool::default();
    let vrf_input = CheckpointVRF::default();
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
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    assert_eq!(dr_hash, DR_HASH.parse().unwrap());
    let dr_epoch = 0;
    dr_pool
        .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
        .unwrap();

    let mut signatures_to_verify = vec![];
    validate_commit_transaction(
        c_tx,
        &dr_pool,
        vrf_input,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
        &utxo_diff,
        collateral_minimum,
        collateral_age,
        block_number,
        minimum_reppoe_difficulty,
        &current_active_wips(),
    )?;
    verify_signatures_test(signatures_to_verify)?;

    Ok(())
}

// Helper function to test a commit with an existing data request,
// but it is very difficult to construct a valid vrf proof
fn test_commit_difficult_proof() {
    let mut dr_pool = DataRequestPool::default();
    let vrf_input = CheckpointVRF::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let secret_key = SecretKey {
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
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
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    assert_eq!(dr_hash, DR_HASH.parse().unwrap());
    let dr_epoch = 0;
    dr_pool
        .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
        .unwrap();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof = DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, dr_hash).unwrap();

    let vto = ValueTransferOutput {
        pkh: cb.proof.proof.pkh(),
        value: ONE_WIT,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 100_000;
    let collateral_minimum = 1;
    let collateral_age = 1;
    let minimum_reppoe_difficulty = 1;
    let utxo_diff = UtxoDiff::new(&utxo_set, 0);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    cb.collateral = vec![vti];
    cb.outputs = vec![];

    // Sign commitment
    let cs = sign_tx(PRIV_KEY_1, &cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    // This test is only valid before the third hard fork
    let active_wips = ActiveWips {
        active_wips: Default::default(),
        block_epoch: 0,
    };

    let mut signatures_to_verify = vec![];
    let x = validate_commit_transaction(
        &c_tx,
        &dr_pool,
        vrf_input,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
        &utxo_diff,
        collateral_minimum,
        collateral_age,
        block_number,
        minimum_reppoe_difficulty,
        &active_wips,
    )
    .and_then(|_| verify_signatures_test(signatures_to_verify));

    match x.unwrap_err().downcast::<TransactionError>().unwrap() {
        TransactionError::DataRequestEligibilityDoesNotMeetTarget { target_hash, .. }
            if target_hash == Hash::with_first_u32(0x003f_ffff) => {}
        e => panic!("{:?}", e),
    }
}

// Helper function to test a commit with an existing data request
// The data requests asks for 1 wit of collateral
fn test_commit_with_collateral(
    utxo_set: &UnspentOutputsPool,
    collateral: (Vec<Input>, Vec<ValueTransferOutput>),
    block_number: u32,
) -> Result<(), failure::Error> {
    let mut signatures_to_verify = vec![];
    let mut dr_pool = DataRequestPool::default();
    let vrf_input = CheckpointVRF::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let secret_key = SecretKey {
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
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
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    assert_eq!(dr_hash, DR_HASH.parse().unwrap());
    let dr_epoch = 0;
    dr_pool
        .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
        .unwrap();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof = DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, dr_hash).unwrap();

    let collateral_minimum = 1;
    let collateral_age = 10;
    let utxo_diff = UtxoDiff::new(utxo_set, 0);
    let minimum_reppoe_difficulty = 1;

    let (inputs, outputs) = collateral;
    cb.collateral = inputs;
    cb.outputs = outputs;

    // Sign commitment
    let cs = sign_tx(PRIV_KEY_1, &cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    validate_commit_transaction(
        &c_tx,
        &dr_pool,
        vrf_input,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
        &utxo_diff,
        collateral_minimum,
        collateral_age,
        block_number,
        minimum_reppoe_difficulty,
        &current_active_wips(),
    )
    .map(|_| ())
}

#[test]
fn commitment_signatures() {
    let dr_hash = DR_HASH.parse().unwrap();
    let vrf_input = CheckpointVRF::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let secret_key = SecretKey {
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
    };
    let mut cb = CommitTransactionBody::default();
    // Insert valid proof
    cb.dr_pointer = dr_hash;
    cb.proof = DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, dr_hash).unwrap();

    let vto = ValueTransferOutput {
        pkh: cb.proof.proof.pkh(),
        value: ONE_WIT,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    cb.collateral = vec![vti];
    cb.outputs = vec![];

    let f = |cb, cs| {
        let c_tx = CommitTransaction::new(cb, vec![cs]);

        test_commit_with_dr_and_utxo_set(&c_tx, &utxo_set)
    };

    let hashable = cb;

    let ks = sign_tx(PRIV_KEY_1, &hashable);
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
            msg: "secp: signature failed verification".to_string(),
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
            expected_pkh: MY_PKH_1.parse().unwrap(),
            signature_pkh,
        }
    );

    // Sign transaction with a different public key
    let ks_different_pk = sign_tx(PRIV_KEY_2, &hashable);
    let signature_pkh = ks_different_pk.public_key.pkh();
    let x = f(hashable, ks_different_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::PublicKeyHashMismatch {
            expected_pkh: MY_PKH_1.parse().unwrap(),
            signature_pkh,
        }
    );
}

#[test]
fn commitment_no_signature() {
    let mut dr_pool = DataRequestPool::default();
    let vrf_input = CheckpointVRF::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
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
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    assert_eq!(dr_hash, DR_HASH.parse().unwrap());
    let dr_epoch = 0;
    dr_pool
        .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
        .unwrap();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof = DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, dr_hash).unwrap();

    let vto = ValueTransferOutput {
        pkh: cb.proof.proof.pkh(),
        value: ONE_WIT,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    cb.collateral = vec![vti];
    cb.outputs = vec![];

    // Do not sign commitment
    let c_tx = CommitTransaction::new(cb, vec![]);

    let x = test_commit_with_dr_and_utxo_set(&c_tx, &utxo_set);
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
    let cs = sign_tx(PRIV_KEY_1, &cb);
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
    let vrf_input = CheckpointVRF::default();

    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let secret_key = SecretKey {
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
    };

    // Create an invalid proof by suppliying a different dr_pointer
    let bad_dr_pointer = Hash::default();
    cb.proof =
        DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, bad_dr_pointer).unwrap();

    let vto = ValueTransferOutput {
        pkh: cb.proof.proof.pkh(),
        value: ONE_WIT,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let block_number = 100_000;
    let collateral_minimum = 1;
    let collateral_age = 1;
    let minimum_reppoe_difficulty = 1;
    let utxo_diff = UtxoDiff::new(&utxo_set, 0);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    cb.collateral = vec![vti];
    cb.outputs = vec![];

    let dro = DataRequestOutput {
        witness_reward: 1000,
        witnesses: 1,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
        ..DataRequestOutput::default()
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_epoch = 0;
    dr_pool
        .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
        .unwrap();

    // Sign commitment
    let cs = sign_tx(PRIV_KEY_1, &cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);
    let mut signatures_to_verify = vec![];

    let x = validate_commit_transaction(
        &c_tx,
        &dr_pool,
        vrf_input,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
        &utxo_diff,
        collateral_minimum,
        collateral_age,
        block_number,
        minimum_reppoe_difficulty,
        &current_active_wips(),
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
    let utxo_set = UnspentOutputsPool::default();
    let block_number = 0;
    let collateral_minimum = 1;
    let collateral_age = 1;
    let minimum_reppoe_difficulty = 1;
    let utxo_diff = UtxoDiff::new(&utxo_set, block_number);

    let mut dr_pool = DataRequestPool::default();
    let block_hash = Hash::default();
    let vrf_input = CheckpointVRF::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let secret_key = SecretKey {
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
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
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    let dr_epoch = 0;
    dr_pool
        .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
        .unwrap();
    dr_pool.update_data_request_stages();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof = DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, dr_hash).unwrap();
    // Sign commitment
    let cs = sign_tx(PRIV_KEY_1, &cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    dr_pool.process_commit(&c_tx, &block_hash).unwrap();
    dr_pool.update_data_request_stages();
    let mut signatures_to_verify = vec![];

    let x = validate_commit_transaction(
        &c_tx,
        &dr_pool,
        vrf_input,
        &mut signatures_to_verify,
        &rep_eng,
        0,
        EpochConstants::default(),
        &utxo_diff,
        collateral_minimum,
        collateral_age,
        block_number,
        minimum_reppoe_difficulty,
        &current_active_wips(),
    );
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NotCommitStage,
    );
}

#[test]
fn commitment_valid() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    let x = test_commit_with_collateral(&utxo_set, (vec![vti], vec![]), 100_000);

    x.unwrap();
}

#[test]
fn commitment_collateral_zero_value_output() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);
    let zero_value_output = ValueTransferOutput {
        pkh: Default::default(),
        value: 0,
        time_lock: 0,
    };

    let x = test_commit_with_collateral(&utxo_set, (vec![vti], vec![zero_value_output]), 100_000);

    let err = x.unwrap_err().downcast::<TransactionError>().unwrap();
    assert!(
        matches!(err, TransactionError::ZeroValueOutput { output_id: 0, .. }),
        "assertion failed: `(left == right)`\n  left: `{:?}`,\n right: `ZeroValueOutput`",
        err
    );
}

#[test]
fn commitment_collateral_output_not_found() {
    let utxo_set = build_utxo_set_with_mint(vec![], None, vec![]);
    let non_existing_output = OutputPointer::default();
    let vti = Input::new(non_existing_output.clone());

    let x = test_commit_with_collateral(&utxo_set, (vec![vti], vec![]), 100_000);

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::OutputNotFound {
            output: non_existing_output
        }
    );
}

#[test]
fn commitment_collateral_pkh_mismatch() {
    let fake_pkh = Default::default();
    let vto = ValueTransferOutput {
        pkh: fake_pkh,
        value: ONE_WIT,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let output = utxo_set.iter().next().unwrap().0;
    let vti = Input::new(output.clone());

    let x = test_commit_with_collateral(&utxo_set, (vec![vti], vec![]), 100_000);

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::CollateralPkhMismatch {
            output,
            output_pkh: fake_pkh,
            proof_pkh: MY_PKH_1.parse().unwrap(),
        }
    );
}

#[test]
fn commitment_mismatched_output_pkh() {
    let fake_pkh = Default::default();
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT + 100,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    let change_output = ValueTransferOutput {
        pkh: fake_pkh,
        value: 100,
        time_lock: 0,
    };

    let x = test_commit_with_collateral(&utxo_set, (vec![vti], vec![change_output]), 100_000);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::PublicKeyHashMismatch {
            expected_pkh: MY_PKH_1.parse().unwrap(),
            signature_pkh: fake_pkh
        }
    );
}

#[test]
fn commitment_several_outputs() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT + 100,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);

    let change_output = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 50,
        time_lock: 0,
    };

    let change_output2 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 50,
        time_lock: 0,
    };

    let x = test_commit_with_collateral(
        &utxo_set,
        (vec![vti], vec![change_output, change_output2]),
        100_000,
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::SeveralCommitOutputs
    );
}

#[test]
fn commitment_collateral_timelocked() {
    let time_lock = 1_000_000_000_000;
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT,
        time_lock,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let output = utxo_set.iter().next().unwrap().0;
    let vti = Input::new(output);

    let x = test_commit_with_collateral(&utxo_set, (vec![vti], vec![]), 100_000);

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::TimeLock {
            current: 0,
            expected: i64::try_from(time_lock).unwrap(),
        }
    );
}

#[test]
fn commitment_collateral_not_mature() {
    let mint_txns = vec![Transaction::Mint(MintTransaction::new(
        TX_COUNTER.fetch_add(1, Ordering::SeqCst),
        vec![ValueTransferOutput {
            pkh: MY_PKH_1.parse().unwrap(),
            value: ONE_WIT,
            time_lock: 0,
        }],
    ))];
    let utxo_set = generate_unspent_outputs_pool(&UnspentOutputsPool::default(), &mint_txns, 1);
    let output = utxo_set.iter().next().unwrap().0;
    let vti = Input::new(output.clone());

    let x = test_commit_with_collateral(&utxo_set, (vec![vti], vec![]), 6);

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::CollateralNotMature {
            must_be_older_than: 10,
            found: 5,
            output,
        }
    );
}

#[test]
fn commitment_collateral_genesis_always_mature() {
    let mint_txns = vec![Transaction::Mint(MintTransaction::new(
        TX_COUNTER.fetch_add(1, Ordering::SeqCst),
        vec![ValueTransferOutput {
            pkh: MY_PKH_1.parse().unwrap(),
            value: ONE_WIT,
            time_lock: 0,
        }],
    ))];
    let utxo_set = generate_unspent_outputs_pool(&UnspentOutputsPool::default(), &mint_txns, 0);
    let output = utxo_set.iter().next().unwrap().0;
    let vti = Input::new(output);

    let x = test_commit_with_collateral(&utxo_set, (vec![vti], vec![]), 5);

    x.unwrap();
}

#[test]
fn commitment_collateral_double_spend() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT / 2,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let output = utxo_set.iter().next().unwrap().0;
    let vti = Input::new(output.clone());

    let x = test_commit_with_collateral(&utxo_set, (vec![vti.clone(), vti], vec![]), 100_000);

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::OutputNotFound { output }
    );
}

#[test]
fn commitment_collateral_wrong_amount() {
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let output = utxo_set.iter().next().unwrap().0;
    let vti = Input::new(output);
    let change_output = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1,
        time_lock: 0,
    };

    let x = test_commit_with_collateral(&utxo_set, (vec![vti], vec![change_output]), 100_000);

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::IncorrectCollateral {
            expected: ONE_WIT,
            found: ONE_WIT - 1
        }
    );
}

#[test]
fn commitment_collateral_negative_amount() {
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT / 2,
        time_lock: 0,
    };
    let vto2 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT / 2,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto1, vto2], None, vec![]);
    let output1 = utxo_set.iter().next().unwrap().0;
    let output2 = utxo_set.iter().nth(1).unwrap().0;
    let inputs = vec![Input::new(output1), Input::new(output2)];
    let change_output = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT + 1,
        time_lock: 0,
    };

    let x = test_commit_with_collateral(&utxo_set, (inputs, vec![change_output]), 100_000);

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeCollateral {
            input_value: ONE_WIT,
            output_value: ONE_WIT + 1
        }
    );
}

#[test]
fn commitment_collateral_zero_is_minimum() {
    let vto1 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT / 2,
        time_lock: 0,
    };
    let vto2 = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT / 2,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto1, vto2], None, vec![]);
    let output1 = utxo_set.iter().next().unwrap().0;
    let output2 = utxo_set.iter().nth(1).unwrap().0;
    let inputs = vec![Input::new(output1), Input::new(output2)];
    let change_output = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: ONE_WIT + 1,
        time_lock: 0,
    };

    let collateral = (inputs, vec![change_output]);
    let x = {
        let mut signatures_to_verify = vec![];
        let mut dr_pool = DataRequestPool::default();
        let vrf_input = CheckpointVRF::default();
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let rep_eng = ReputationEngine::new(100);
        let secret_key = SecretKey {
            bytes: Protected::from(PRIV_KEY_1.to_vec()),
        };

        let dro = DataRequestOutput {
            witness_reward: 1000,
            witnesses: 1,
            min_consensus_percentage: 51,
            data_request: example_data_request(),
            collateral: 0,
            ..DataRequestOutput::default()
        };
        let dr_body = DRTransactionBody::new(vec![], vec![], dro);
        let drs = sign_tx(PRIV_KEY_1, &dr_body);
        let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
        let dr_hash = dr_transaction.hash();
        // dr_hash changed because the collateral is 0
        assert_eq!(
            dr_hash,
            "0c2775673255e90167230d5e70b2b087fa66e45c69e46744d7516e89ca764076"
                .parse()
                .unwrap()
        );
        let dr_epoch = 0;
        dr_pool
            .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
            .unwrap();

        // Insert valid proof
        let mut cb = CommitTransactionBody::default();
        cb.dr_pointer = dr_hash;
        cb.proof =
            DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, dr_hash).unwrap();

        let block_number = 100_000;
        let collateral_minimum = 1;
        let collateral_age = 1;
        let minimum_reppoe_difficulty = 1;
        let utxo_diff = UtxoDiff::new(&utxo_set, 0);

        let (inputs, outputs) = collateral;
        cb.collateral = inputs;
        cb.outputs = outputs;

        // Sign commitment
        let cs = sign_tx(PRIV_KEY_1, &cb);
        let c_tx = CommitTransaction::new(cb, vec![cs]);

        validate_commit_transaction(
            &c_tx,
            &dr_pool,
            vrf_input,
            &mut signatures_to_verify,
            &rep_eng,
            0,
            EpochConstants::default(),
            &utxo_diff,
            collateral_minimum,
            collateral_age,
            block_number,
            minimum_reppoe_difficulty,
            &current_active_wips(),
        )
        .map(|_| ())
    };
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::NegativeCollateral {
            input_value: ONE_WIT,
            output_value: ONE_WIT + 1
        }
    );
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
        let vrf_input = CheckpointVRF::default();
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let rep_eng = ReputationEngine::new(100);
        let secret_key = SecretKey {
            bytes: Protected::from(PRIV_KEY_1.to_vec()),
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
        let drs = sign_tx(PRIV_KEY_1, &dr_body);
        let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
        let dr_hash = dr_transaction.hash();
        let dr_epoch = 0;
        dr_pool
            .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
            .unwrap();

        // Insert valid proof
        let mut cb = CommitTransactionBody::default();
        cb.dr_pointer = dr_hash;
        cb.proof =
            DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, dr_hash).unwrap();

        let vto = ValueTransferOutput {
            pkh: cb.proof.proof.pkh(),
            value: ONE_WIT,
            time_lock: 0,
        };
        let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
        let block_number = 100_000;
        let collateral_minimum = 1;
        let collateral_age = 1;
        let minimum_reppoe_difficulty = 1;
        let utxo_diff = UtxoDiff::new(&utxo_set, 0);
        let vti = Input::new(utxo_set.iter().next().unwrap().0);

        cb.collateral = vec![vti];
        cb.outputs = vec![];

        // Sign commitment
        let cs = sign_tx(PRIV_KEY_1, &cb);
        let c_tx = CommitTransaction::new(cb, vec![cs]);
        // This test checks that the first hard fork is handled correctly
        let active_wips = active_wips_from_mainnet(epoch);

        let mut signatures_to_verify = vec![];
        validate_commit_transaction(
            &c_tx,
            &dr_pool,
            vrf_input,
            &mut signatures_to_verify,
            &rep_eng,
            epoch,
            epoch_constants,
            &utxo_diff,
            collateral_minimum,
            collateral_age,
            block_number,
            minimum_reppoe_difficulty,
            &active_wips,
        )
        .map(|_| ())?;

        verify_signatures_test(signatures_to_verify)
    };

    let first_timestamp =
        u64::from(FIRST_HARD_FORK) * u64::from(epoch_constants.checkpoints_period);

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
        // After FIRST_HARD_FORK epoch, this validation is disabled
        (FIRST_HARD_FORK - 1, first_timestamp + 1_000_000, false),
        (FIRST_HARD_FORK, first_timestamp, true),
        (FIRST_HARD_FORK, first_timestamp + 1_000_000, true),
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
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_pointer = dr_transaction.hash();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_pointer;
    // Sign commitment
    let cs = sign_tx(PRIV_KEY_1, &cb);
    let c_tx = CommitTransaction::new(cb, vec![cs]);

    dr_pool
        .process_data_request(&dr_transaction, epoch, &Hash::default())
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
    rb.pkh = MY_PKH_1.parse().unwrap();

    let f = |rb, rs| -> Result<_, failure::Error> {
        let r_tx = RevealTransaction::new(rb, vec![rs]);
        let mut signatures_to_verify = vec![];
        let ret = validate_reveal_transaction(&r_tx, &dr_pool, &mut signatures_to_verify)?;
        verify_signatures_test(signatures_to_verify)?;
        Ok(ret)
    };

    let hashable = rb;

    let ks = sign_tx(PRIV_KEY_1, &hashable);
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
            expected_pkh: MY_PKH_1.parse().unwrap(),
            signature_pkh,
        }
    );

    // Sign transaction with a different public key
    let ks_different_pk = sign_tx(PRIV_KEY_2, &hashable);
    let signature_pkh = ks_different_pk.public_key.pkh();
    let x = f(hashable, ks_different_pk);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::PublicKeyHashMismatch {
            expected_pkh: MY_PKH_1.parse().unwrap(),
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
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_pointer = dr_transaction.hash();
    dr_pool
        .add_data_request(epoch, dr_transaction, &Hash::default())
        .unwrap();

    let mut rb = RevealTransactionBody::default();
    rb.dr_pointer = dr_pointer;
    let rs = sign_tx(PRIV_KEY_1, &rb);
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
    let rs = sign_tx(PRIV_KEY_1, &rb);
    let r_tx = RevealTransaction::new(rb, vec![rs]);

    let x = validate_reveal_transaction(&r_tx, &dr_pool, &mut signatures_to_verify);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::PublicKeyHashMismatch {
            expected_pkh: bad_pkh,
            signature_pkh: MY_PKH_1.parse().unwrap(),
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
    let rs = sign_tx(PRIV_KEY_1, &rb);
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
    let rs = sign_tx(PRIV_KEY_2, &rb);
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
    rb.pkh = MY_PKH_1.parse().unwrap();
    let rs = sign_tx(PRIV_KEY_1, &rb);
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
    let mut dr_pool = DataRequestPool::new(2);

    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let dr_output = DataRequestOutput {
        witnesses: 5,
        commit_and_reveal_fee: 20,
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
        .process_data_request(&dr_transaction, epoch, &Hash::default())
        .unwrap();
    dr_pool.update_data_request_stages();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_tx(PRIV_KEY_1, &RevealTransactionBody::default()).public_key;

    // Create Reveal and Commit
    let reveal_body = RevealTransactionBody::new(dr_pointer, vec![], public_key.pkh());
    let reveal_signature = sign_tx(PRIV_KEY_1, &reveal_body);
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
    let reveal_signature2 = sign_tx(PRIV_KEY_1, &reveal_body2);
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

// Auxiliar function to create a pair commit and reveal
// It also returns the PublicKey of the committer
fn create_commit_reveal(
    mk: [u8; 32],
    dr_pointer: Hash,
    reveal_value: Vec<u8>,
) -> (PublicKey, CommitTransaction, RevealTransaction) {
    let public_key = sign_tx(mk, &RevealTransactionBody::default()).public_key;

    let reveal_body = RevealTransactionBody::new(dr_pointer, reveal_value, public_key.pkh());
    let reveal_signature = sign_tx(mk, &reveal_body);
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

    (public_key, commit_transaction, reveal_transaction)
}

// Auxiliar function to create all the commits and reveals specified
// It returns:
//   - A CommitTransaction vector
//   - A RevealTransaction vector
//   - A vector with the liar publicKeys
//   - A vector with the error publicKeys
#[allow(clippy::cast_possible_truncation, clippy::too_many_arguments)]
fn create_commits_reveals(
    dr_pointer: Hash,
    commits_count: usize,
    reveal_values: Vec<Vec<u8>>,
    liar_values: Vec<Vec<u8>>,
    error_values: Vec<Vec<u8>>,
    dr_mk: Option<[u8; 32]>,
    dr_liar: bool,
) -> (
    Vec<CommitTransaction>,
    Vec<RevealTransaction>,
    Vec<PublicKey>,
    Vec<PublicKey>,
) {
    let liars_count = liar_values.len();
    let errors_count = error_values.len();
    let mut reveal_value = reveal_values.into_iter();
    let mut liar_value = liar_values.into_iter();
    let mut error_value = error_values.into_iter();
    let mut commits = vec![];
    let mut reveals = vec![];
    let mut liars = vec![];
    let mut errors = vec![];
    let mut commits_count = commits_count;
    let mut liars_count = liars_count;
    let mut errors_count = errors_count;

    if dr_mk.is_some() && dr_liar && liars_count == 0 {
        panic!("Data requester can not lie if liars_count is equal to 0");
    }
    if liars_count + errors_count > commits_count {
        panic!("Liars number plus errors number can not be bigger than commits number");
    }
    if commits_count > u8::max_value() as usize {
        panic!("High commits number produces overflow in the test");
    }

    // Handle data requester committer case
    if let Some(dr_mk) = dr_mk {
        if commits_count > 0 {
            let reveal_value = if dr_liar {
                liars_count -= 1;
                liar_value.next().unwrap()
            } else {
                reveal_value.next().unwrap()
            };

            let (dr_public_key, commit, reveal) =
                create_commit_reveal(dr_mk, dr_pointer, reveal_value);
            commits_count -= 1;
            commits.push(commit);
            reveals.push(reveal);
            if dr_liar {
                liars.push(dr_public_key)
            }
        }
    }

    // Create the rest of commits and reveals
    for i in 0..commits_count {
        let reveal_value = if liars_count > 0 {
            liar_value.next().unwrap()
        } else if errors_count > 0 {
            error_value.next().unwrap()
        } else {
            reveal_value.next().unwrap()
        };
        let index = i as u8 + 1;
        let (public_key, commit, reveal) =
            create_commit_reveal([index as u8; 32], dr_pointer, reveal_value);
        commits.push(commit);
        reveals.push(reveal);
        if liars_count > 0 {
            liars_count -= 1;
            liars.push(public_key.clone())
        } else if errors_count > 0 {
            errors_count -= 1;
            errors.push(public_key.clone())
        }
    }

    (commits, reveals, liars, errors)
}

fn include_commits(
    dr_pool: &mut DataRequestPool,
    commits_count: usize,
    commits: Vec<CommitTransaction>,
) {
    assert!(commits_count <= commits.len());

    let fake_block_hash = Hash::SHA256([1; 32]);
    for commit in commits.iter().take(commits_count) {
        dr_pool.process_commit(commit, &fake_block_hash).unwrap();
    }
    dr_pool.update_data_request_stages();
}

fn include_reveals(
    dr_pool: &mut DataRequestPool,
    reveals_count: usize,
    reveals: Vec<RevealTransaction>,
) {
    assert!(reveals_count <= reveals.len());

    let fake_block_hash = Hash::SHA256([2; 32]);
    for reveal in reveals.iter().take(reveals_count) {
        dr_pool.process_reveal(reveal, &fake_block_hash).unwrap();
    }
    dr_pool.update_data_request_stages();
}

fn get_rewarded_and_slashed(
    reveals_count: usize,
    liars: Vec<PublicKey>,
    errors: Vec<PublicKey>,
    commits: Vec<CommitTransaction>,
) -> (Vec<PublicKeyHash>, Vec<PublicKeyHash>, Vec<PublicKeyHash>) {
    let mut slashed_pkhs = vec![];
    let mut rewarded_pkhs = vec![];
    let mut errors_pkhs = vec![];

    // Slash liars and reward honest
    for (i, commit) in commits.iter().enumerate() {
        if i >= reveals_count {
            // All the commits without reveals will be slashed
            slashed_pkhs.push(commits[i].signatures[0].public_key.pkh());
        }
        // Liars are in the beginning of commits vector
        if i < liars.len() {
            slashed_pkhs.push(commit.signatures[0].public_key.pkh());
        // Errors are in the second position of commits vector
        } else if i < liars.len() + errors.len() {
            errors_pkhs.push(commit.signatures[0].public_key.pkh());
        } else {
            rewarded_pkhs.push(commit.signatures[0].public_key.pkh());
        }
    }

    (rewarded_pkhs, slashed_pkhs, errors_pkhs)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dr_pool_with_dr_in_tally_all_errors(
    dr_output: DataRequestOutput,
    commits_count: usize, // Commits number included in DataRequestPool
    reveals_count: usize, // Reveals number included in DataRequestPool
    error_value: Vec<u8>, // Error reveal value
) -> (
    DataRequestPool,    // DataRequestPool updated
    Hash,               // Data Request pointer
    Vec<PublicKeyHash>, // Rewarded witnesses
    Vec<PublicKeyHash>, // Slashed witnesses
    Vec<PublicKeyHash>, // Error witnesses
    PublicKeyHash,      // Data Requester
    u64,                // Tally change value
    u64,                // Witnesses reward value
) {
    assert!(
        commits_count >= reveals_count,
        "Reveals count cannot be greater than commits count"
    );

    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create Data Requester public key
    let dr_mk = [0xBB; 32];
    let dr_public_key = sign_tx(dr_mk, &RevealTransactionBody::default()).public_key;

    // Create DRTransaction
    let epoch = 0;
    let dr_transaction = DRTransaction {
        body: DRTransactionBody::new(vec![], vec![], dr_output.clone()),
        signatures: vec![KeyedSignature {
            signature: Default::default(),
            public_key: dr_public_key.clone(),
        }],
    };
    let dr_pointer = dr_transaction.hash();

    // Create requested commits and reveals
    let dr_mk = None;
    let liars_count = 0;
    let errors_count = reveals_count;
    let reveal_value = vec![];
    let liar_value = vec![];
    let dr_liar = false;
    let (commits, reveals, liars, errors) = create_commits_reveals(
        dr_pointer,
        commits_count,
        vec![reveal_value; commits_count],
        vec![liar_value; liars_count],
        vec![error_value; errors_count],
        dr_mk,
        dr_liar,
    );

    // Include DRTransaction in DataRequestPool
    dr_pool
        .process_data_request(&dr_transaction, epoch, &Hash::default())
        .unwrap();
    dr_pool.update_data_request_stages();

    // Include commits and reveals in DataRequestPool
    include_commits(&mut dr_pool, commits_count, commits.clone());
    include_reveals(&mut dr_pool, reveals_count, reveals);

    // Create vector of rewarded and slashed public key hashes
    let (rewarded, slashed, error_witnesses) =
        get_rewarded_and_slashed(reveals_count, liars, errors, commits);

    // Calculate tally change assuming that the consensus will be error, and therefore errors will
    // be rewarded
    let change = calculate_tally_change(commits_count, reveals_count, reveals_count, &dr_output);

    // To calculate witness reward we take into account than non-revealers are considered liars
    let liars_count = liars_count + commits_count - reveals_count;
    // Calculate tally change assuming that the consensus will be error, and therefore errors will
    // be rewarded
    let (reward, _) = calculate_witness_reward(
        commits_count,
        liars_count,
        0,
        dr_output.witness_reward,
        ONE_WIT,
    );

    (
        dr_pool,
        dr_pointer,
        rewarded,
        slashed,
        error_witnesses,
        dr_public_key.pkh(),
        change,
        reward,
    )
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dr_pool_with_dr_in_tally_stage_generic(
    dr_output: DataRequestOutput,
    commits_count: usize,        // Commits number included in DataRequestPool
    reveal_values: Vec<Vec<u8>>, // Honest reveal values
    liar_values: Vec<Vec<u8>>,   // Dishonest reveal values
    error_values: Vec<Vec<u8>>,  // Error reveal values
    dr_committer: bool,          // Flag to indicate that the data requester is also a committer
    dr_liar: bool,               // Flag to indicate that the data requester lies
) -> (
    DataRequestPool,    // DataRequestPool updated
    Hash,               // Data Request pointer
    Vec<PublicKeyHash>, // Rewarded witnesses
    Vec<PublicKeyHash>, // Slashed witnesses
    Vec<PublicKeyHash>, // Error witnesses
    PublicKeyHash,      // Data Requester
    u64,                // Tally change value
    u64,                // Witnesses reward value
) {
    let reveals_count = reveal_values.len();
    let liars_count = liar_values.len();
    let errors_count = error_values.len();

    if !dr_committer && dr_liar {
        panic!("Data requester can not lie if he can not commit");
    }
    if liars_count > reveals_count {
        panic!("Liars number can not be bigger than reveals number");
    }
    if reveals_count - liars_count - errors_count < liars_count + errors_count {
        panic!("Honest committers should be more than liars and errors");
    }

    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create Data Requester public key
    let dr_mk = [0xBB; 32];
    let dr_public_key = sign_tx(dr_mk, &RevealTransactionBody::default()).public_key;

    // Create DRTransaction
    let epoch = 0;
    let dr_transaction = DRTransaction {
        body: DRTransactionBody::new(vec![], vec![], dr_output.clone()),
        signatures: vec![KeyedSignature {
            signature: Default::default(),
            public_key: dr_public_key.clone(),
        }],
    };
    let dr_pointer = dr_transaction.hash();

    // Create requested commits and reveals
    let dr_mk = if dr_committer { Some(dr_mk) } else { None };
    // When reveals_count < commits_count, we need to add some dummy reveal values
    let mut reveal_values = reveal_values;
    for _ in reveals_count..commits_count {
        reveal_values.push(vec![]);
    }
    let (commits, reveals, liars, errors) = create_commits_reveals(
        dr_pointer,
        commits_count,
        reveal_values,
        liar_values,
        error_values,
        dr_mk,
        dr_liar,
    );

    // Include DRTransaction in DataRequestPool
    dr_pool
        .process_data_request(&dr_transaction, epoch, &Hash::default())
        .unwrap();
    dr_pool.update_data_request_stages();

    // Include commits and reveals in DataRequestPool
    include_commits(&mut dr_pool, commits_count, commits.clone());
    include_reveals(&mut dr_pool, reveals_count, reveals);

    // Create vector of rewarded and slashed public key hashes
    let (rewarded, slashed, error_witnesses) =
        get_rewarded_and_slashed(reveals_count, liars, errors, commits);

    // TODO: here liars_count must be equal to reveals_count if the result of the data request is
    // "InsufficientConsensus"
    // Calculate tally change
    let change = calculate_tally_change(
        commits_count,
        reveals_count,
        reveals_count - liars_count - errors_count,
        &dr_output,
    );

    // To calculate witness reward we take into account than non-revealers are considered liars
    let liars_count = liars_count + commits_count - reveals_count;
    // Calculate witness reward
    let (reward, _) = calculate_witness_reward(
        commits_count,
        liars_count,
        errors_count,
        dr_output.witness_reward,
        ONE_WIT,
    );

    (
        dr_pool,
        dr_pointer,
        rewarded,
        slashed,
        error_witnesses,
        dr_public_key.pkh(),
        change,
        reward,
    )
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dr_pool_with_dr_in_tally_stage(
    dr_output: DataRequestOutput,
    commits_count: usize,
    reveals_count: usize,
    liars_count: usize,
    reveal_value: Vec<u8>,
    liar_value: Vec<u8>,
) -> (
    DataRequestPool,
    Hash,
    Vec<PublicKeyHash>,
    Vec<PublicKeyHash>,
    Vec<PublicKeyHash>,
    PublicKeyHash,
    u64,
    u64,
) {
    dr_pool_with_dr_in_tally_stage_generic(
        dr_output,
        commits_count,
        vec![reveal_value; reveals_count],
        vec![liar_value; liars_count],
        vec![],
        false,
        false,
    )
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dr_pool_with_dr_in_tally_stage_different_reveals(
    dr_output: DataRequestOutput,
    commits_count: usize,
    reveal_values: Vec<Vec<u8>>,
    liar_values: Vec<Vec<u8>>,
) -> (
    DataRequestPool,
    Hash,
    Vec<PublicKeyHash>,
    Vec<PublicKeyHash>,
    Vec<PublicKeyHash>,
    PublicKeyHash,
    u64,
    u64,
) {
    dr_pool_with_dr_in_tally_stage_generic(
        dr_output,
        commits_count,
        reveal_values,
        liar_values,
        vec![],
        false,
        false,
    )
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dr_pool_with_dr_in_tally_stage_with_errors(
    dr_output: DataRequestOutput,
    commits_count: usize,
    reveals_count: usize,
    liars_count: usize,
    errors_count: usize,
    reveal_value: Vec<u8>,
    liar_value: Vec<u8>,
) -> (
    DataRequestPool,
    Hash,
    Vec<PublicKeyHash>,
    Vec<PublicKeyHash>,
    Vec<PublicKeyHash>,
    PublicKeyHash,
    u64,
    u64,
) {
    let error_value = RadonReport::from_result(
        Ok(RadonTypes::from(
            RadonError::try_from(RadError::RetrieveTimeout).unwrap(),
        )),
        &ReportContext::default(),
    )
    .result
    .encode()
    .unwrap();

    dr_pool_with_dr_in_tally_stage_generic(
        dr_output,
        commits_count,
        vec![reveal_value; reveals_count],
        vec![liar_value; liars_count],
        vec![error_value; errors_count],
        false,
        false,
    )
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn dr_pool_with_dr_in_tally_stage_with_dr_liar(
    dr_output: DataRequestOutput,
    commits_count: usize,
    reveals_count: usize,
    liars_count: usize,
    reveal_value: Vec<u8>,
    liar_value: Vec<u8>,
) -> (
    DataRequestPool,
    Hash,
    Vec<PublicKeyHash>,
    Vec<PublicKeyHash>,
    Vec<PublicKeyHash>,
    PublicKeyHash,
    u64,
    u64,
) {
    dr_pool_with_dr_in_tally_stage_generic(
        dr_output,
        commits_count,
        vec![reveal_value; reveals_count],
        vec![liar_value; liars_count],
        vec![],
        true,
        true,
    )
}

#[test]
fn tally_dr_not_tally_stage() {
    // Create DRTransaction
    let fake_block_hash = Hash::SHA256([1; 32]);
    let epoch = 0;
    let active_wips = current_active_wips();
    let dr_output = DataRequestOutput {
        witnesses: 1,
        commit_and_reveal_fee: 20,
        witness_reward: 1000,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
    };
    let dr_transaction_body = DRTransactionBody::new(vec![], vec![], dr_output.clone());
    let dr_transaction_signature = sign_tx(PRIV_KEY_2, &dr_transaction_body);
    let dr_transaction = DRTransaction::new(dr_transaction_body, vec![dr_transaction_signature]);
    let dr_pointer = dr_transaction.hash();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_tx(PRIV_KEY_1, &RevealTransactionBody::default()).public_key;

    // Create Reveal and Commit
    // Reveal = integer(0)
    let reveal_value = vec![0x00];
    let reveal_body = RevealTransactionBody::new(dr_pointer, reveal_value, public_key.pkh());
    let reveal_signature = sign_tx(PRIV_KEY_1, &reveal_body);
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
    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: public_key.pkh(),
        value: dr_output.witness_reward + dr_output.collateral,
    };
    let tally_transaction =
        TallyTransaction::new(dr_pointer, tally_value, vec![vt0], vec![], vec![]);

    let mut dr_pool = DataRequestPool::default();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips);
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DataRequestNotFound { hash: dr_pointer },
    );
    dr_pool
        .process_data_request(&dr_transaction, epoch, &Hash::default())
        .unwrap();
    dr_pool.update_data_request_stages();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips);
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NotTallyStage
    );

    dr_pool
        .process_commit(&commit_transaction, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();
    let x = validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips);
    assert_eq!(
        x.unwrap_err().downcast::<DataRequestError>().unwrap(),
        DataRequestError::NotTallyStage
    );

    dr_pool
        .process_reveal(&reveal_transaction, &fake_block_hash)
        .unwrap();
    dr_pool.update_data_request_stages();
    let x =
        validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips).map(|_| ());
    x.unwrap();
}

#[test]
fn tally_invalid_consensus() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];

    let dr_output = example_data_request_output(5, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, _dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 5, 4, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT + (ONE_WIT / 4));

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    // Fake tally value: integer(1)
    let fake_tally_value = vec![0x01];

    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[3],
        value: reward,
    };
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: PublicKeyHash::default(),
        value: change,
    };

    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        fake_tally_value.clone(),
        vec![vt0, vt1, vt2, vt3, vt_change],
        slashed,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchedConsensus {
            expected_tally: tally_value,
            miner_tally: fake_tally_value,
        }
    );
}

#[test]
fn tally_valid_1_reveal_5_commits() {
    let collateral = ONE_WIT;
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test(5, vec![vec![0]]);
    let change = 5 * 200 + 4 * 20;

    let tally_value = RadonReport::from_result(
        Ok(RadonTypes::from(
            RadonError::try_from(RadError::InsufficientConsensus {
                achieved: 0.2,
                required: 0.51,
            })
            .unwrap(),
        )),
        &ReportContext::default(),
    );
    let tally_bytes = tally_value.into_inner().encode().unwrap();

    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[4],
        value: collateral,
    };
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };

    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_bytes,
        vec![vt0, vt1, vt2, vt3, vt4, vt_change],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3], pkhs[4]],
        vec![pkhs[0]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

fn generic_tally_test_stddev_dr(
    num_commits: usize,
    reveals: Vec<Vec<u8>>,
    stddev_cbor: Vec<u8>,
) -> (Vec<PublicKeyHash>, PublicKeyHash, Hash, DataRequestPool) {
    let data_request = RADRequest {
        time_lock: 0,
        retrieve: vec![RADRetrieve {
            kind: RADType::HttpGet,
            url: "".to_string(),
            script: vec![0x80],
            body: vec![],
            headers: vec![],
        }],
        aggregate: RADAggregate {
            filters: vec![],
            reducer: RadonReducers::AverageMean as u32,
        },
        tally: RADTally {
            filters: vec![RADFilter {
                op: RadonFilters::DeviationStandard as u32,
                args: stddev_cbor,
            }],
            reducer: RadonReducers::AverageMean as u32,
        },
    };

    let collateral = ONE_WIT;
    let dr_output = DataRequestOutput {
        witnesses: 4,
        commit_and_reveal_fee: 10,
        witness_reward: 1000,
        min_consensus_percentage: 51,
        data_request,
        collateral,
    };

    generic_tally_test_inner(num_commits, reveals, dr_output)
}

fn generic_tally_test_rng(
    num_commits: usize,
    reveals: Vec<Vec<u8>>,
) -> (Vec<PublicKeyHash>, PublicKeyHash, Hash, DataRequestPool) {
    let dr_output = example_data_request_output_rng(u16::try_from(num_commits).unwrap(), 200, 20);

    generic_tally_test_inner(num_commits, reveals, dr_output)
}

fn generic_tally_test(
    num_commits: usize,
    reveals: Vec<Vec<u8>>,
) -> (Vec<PublicKeyHash>, PublicKeyHash, Hash, DataRequestPool) {
    let dr_output = example_data_request_output(u16::try_from(num_commits).unwrap(), 200, 20);

    generic_tally_test_inner(num_commits, reveals, dr_output)
}

fn generic_tally_test_inner(
    num_commits: usize,
    reveals: Vec<Vec<u8>>,
    dr_output: DataRequestOutput,
) -> (Vec<PublicKeyHash>, PublicKeyHash, Hash, DataRequestPool) {
    assert!(num_commits >= reveals.len());

    // Create DataRequestPool
    let mut dr_pool = DataRequestPool::default();

    // Create Data Requester public key
    let dr_mk = [0xBB; 32];
    let dr_public_key = sign_tx(dr_mk, &RevealTransactionBody::default()).public_key;

    // Create DRTransaction
    let epoch = 0;
    let dr_transaction = DRTransaction {
        body: DRTransactionBody::new(vec![], vec![], dr_output),
        signatures: vec![KeyedSignature {
            signature: Default::default(),
            public_key: dr_public_key.clone(),
        }],
    };
    let dr_pointer = dr_transaction.hash();

    // Create requested commits and reveals
    let commits_count = num_commits;
    let reveals_count = reveals.len();
    let reveal_value = reveals;
    let mut commits = vec![];
    let mut reveals = vec![];
    let mut pkhs = vec![];

    for index in 0..commits_count {
        let (public_key, commit, reveal) = create_commit_reveal(
            [u8::try_from(index + 1).unwrap(); 32],
            dr_pointer,
            reveal_value.get(index).cloned().unwrap_or_default(),
        );
        commits.push(commit);
        reveals.push(reveal);
        pkhs.push(public_key.pkh());
    }

    // Include DRTransaction in DataRequestPool
    dr_pool
        .process_data_request(&dr_transaction, epoch, &Hash::default())
        .unwrap();
    dr_pool.update_data_request_stages();

    // Include commits and reveals in DataRequestPool
    include_commits(&mut dr_pool, commits_count, commits);
    include_reveals(&mut dr_pool, reveals_count, reveals);

    let dr_pkh = dr_public_key.pkh();

    (pkhs, dr_pkh, dr_pointer, dr_pool)
}

#[test]
fn tally_valid_1_reveal_5_commits_invalid_value() {
    let collateral = ONE_WIT;
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test(5, vec![vec![0]]);
    let change = 5 * 200 + 4 * 20;

    let tally_value = RadonReport::from_result(
        Ok(RadonTypes::from(
            RadonError::try_from(RadError::InsufficientConsensus {
                achieved: 0.2,
                required: 0.51,
            })
            .unwrap(),
        )),
        &ReportContext::default(),
    );
    let tally_bytes = tally_value.into_inner().encode().unwrap();

    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral * 4 - 3,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: 1,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: 1,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[4],
        value: 1,
    };
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };

    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_bytes,
        vec![vt0, vt1, vt2, vt3, vt4, vt_change],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3], pkhs[4]],
        vec![pkhs[0]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidReward {
            value: collateral * 4 - 3,
            expected_value: collateral
        }
    );
}

#[test]
fn tally_valid_1_reveal_5_commits_with_absurd_timelock() {
    let collateral = ONE_WIT;
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test(5, vec![vec![0]]);
    let change = 5 * 200 + 4 * 20;

    let tally_value = RadonReport::from_result(
        Ok(RadonTypes::from(
            RadonError::try_from(RadError::InsufficientConsensus {
                achieved: 0.2,
                required: 0.51,
            })
            .unwrap(),
        )),
        &ReportContext::default(),
    );
    let tally_bytes = tally_value.into_inner().encode().unwrap();

    let vt0 = ValueTransferOutput {
        time_lock: u64::MAX,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: u64::MAX,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: u64::MAX,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: u64::MAX,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: u64::MAX,
        pkh: pkhs[4],
        value: collateral,
    };
    let vt_change = ValueTransferOutput {
        time_lock: u64::MAX,
        pkh: dr_pkh,
        value: change,
    };

    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_bytes,
        vec![vt0, vt1, vt2, vt3, vt4, vt_change],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3], pkhs[4]],
        vec![pkhs[0]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidTimeLock {
            expected: 0,
            current: u64::MAX,
        }
    );
}

#[test]
fn tally_valid() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let dr_output = example_data_request_output(5, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 5, 4, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT + (ONE_WIT / 4));

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[3],
        value: reward,
    };
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt_change],
        slashed,
        error_witnesses,
    );

    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_too_many_outputs() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let dr_output = example_data_request_output(5, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 5, 4, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT + (ONE_WIT / 4));

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt_change],
        slashed,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::WrongNumberOutputs {
            outputs: tally_transaction.outputs.len(),
            expected_outputs: 5
        },
    );
}

#[test]
fn tally_too_less_outputs() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let dr_output = example_data_request_output(2, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, _dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 2, 2, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };

    let tally_transaction =
        TallyTransaction::new(dr_pointer, tally_value, vec![vt0], slashed, error_witnesses);
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
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
    let dr_output = example_data_request_output(5, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 5, 4, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT + (ONE_WIT / 4));

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[3],
        value: reward,
    };
    let invalid_change = 1000;
    let vt_change = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: invalid_change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt_change],
        slashed,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidTallyChange {
            change: invalid_change,
            expected_change: change
        },
    );
}

#[test]
fn tally_double_reward() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let dr_output = example_data_request_output(2, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, _dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 2, 2, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1],
        slashed,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MultipleRewards { pkh: rewarded[0] },
    );
}

#[test]
fn tally_reveal_not_found() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let dr_output = example_data_request_output(2, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, _dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 2, 2, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: PublicKeyHash::default(),
        value: reward,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1],
        slashed,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::RevealNotFound,
    );
}

#[test]
fn tally_invalid_reward() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let dr_output = example_data_request_output(2, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, _dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 2, 2, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward + 1000,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    assert_eq!(change, 0);
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1],
        slashed,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidReward {
            value: reward + 1000,
            expected_value: reward
        },
    );
}

#[test]
fn tally_valid_2_reveals() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let dr_output = example_data_request_output(2, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, _dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 2, 2, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    assert_eq!(change, 0);
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1],
        slashed,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_3_reveals_dr_liar() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let liar_value = vec![0x0a];

    // Create a DataRequestPool with 3 reveals (one of them is a lie from the data requester)
    let dr_output = example_data_request_output_with_mode_filter(3, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage_with_dr_liar(dr_output, 3, 3, 1, reveal_value, liar_value);
    assert_eq!(reward, 200 + ONE_WIT + ONE_WIT / 2);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    assert_eq!(slashed, vec![dr_pkh]);
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2],
        slashed,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_3_reveals_dr_liar_invalid() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let liar_value = vec![0x0a];

    // Create a DataRequestPool with 3 reveals (one of them is a lie from the data requester)
    let dr_output = example_data_request_output_with_mode_filter(3, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage_with_dr_liar(dr_output, 3, 3, 1, reveal_value, liar_value);
    assert_eq!(reward, 200 + ONE_WIT + ONE_WIT / 2);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: reward,
    };
    let slashed_witnesses = vec![];
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2],
        slashed_witnesses.clone(),
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchingOutOfConsensusCount {
            expected: slashed.into_iter().sorted().collect(),
            found: slashed_witnesses.into_iter().sorted().collect(),
        },
    );
}

#[test]
fn tally_valid_5_reveals_1_liar_1_error() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let liar_value = vec![0x0a];

    // Create a DataRequestPool with 5 reveals (one of them is a lie and another is an error)
    let dr_output = example_data_request_output_with_mode_filter(5, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage_with_errors(dr_output, 5, 5, 1, 1, reveal_value, liar_value);
    assert_eq!(reward, 200 + ONE_WIT + ONE_WIT / 3);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: error_witnesses[0],
        value: ONE_WIT,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    assert_eq!(slashed.len(), 1);
    assert_eq!(error_witnesses.len(), 1);

    let mut out_of_consensus = slashed;
    out_of_consensus.extend(error_witnesses.clone());
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        out_of_consensus,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_3_reveals_1_error() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];

    // Create a DataRequestPool with 3 reveals (one of them is a lie from the data requester)
    let dr_output = example_data_request_output_with_mode_filter(3, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage_with_errors(dr_output, 3, 3, 0, 1, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: error_witnesses[0],
        value: ONE_WIT,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    assert_eq!(slashed, vec![]);
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3],
        error_witnesses.clone(),
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_3_reveals_1_error_invalid_reward() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];

    // Create a DataRequestPool with 3 reveals (one of them is a lie from the data requester)
    let dr_output = example_data_request_output_with_mode_filter(3, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage_with_errors(dr_output, 3, 3, 0, 1, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: error_witnesses[0],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    assert_eq!(slashed, vec![]);
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3],
        error_witnesses.clone(),
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::InvalidReward {
            value: reward,
            expected_value: ONE_WIT,
        }
    )
}

#[test]
fn tally_valid_3_reveals_mark_all_as_error() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];

    // Create a DataRequestPool with 3 reveals (one of them is a lie from the data requester)
    let dr_output = example_data_request_output_with_mode_filter(3, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage_with_errors(dr_output, 3, 3, 0, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    assert_eq!(slashed, vec![]);
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3],
        error_witnesses,
        vec![rewarded[0], rewarded[1], rewarded[2]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchingErrorCount {
            expected: vec![],
            found: vec![rewarded[2], rewarded[1], rewarded[0]],
        }
    )
}

#[test]
fn tally_dishonest_reward() {
    // Reveal value: integer(0)
    let reveal_value = vec![0x00];
    let liar_value = vec![0x0a];

    // Create a DataRequestPool with 3 reveals (one of them is a lie from the data requester)
    let dr_output = example_data_request_output_with_mode_filter(3, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage_with_dr_liar(dr_output, 3, 3, 1, reveal_value, liar_value);
    assert_eq!(reward, 200 + ONE_WIT + ONE_WIT / 2);

    // Tally value: integer(0)
    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: reward,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2],
        slashed,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DishonestReward,
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
    let dr_output = example_data_request_output_with_mode_filter(3, 200, 20);
    let (dr_pool, dr_pointer, rewarded, _slashed, _error_witnesses, dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage_with_dr_liar(
            dr_output.clone(),
            3,
            3,
            1,
            reveal_value.result.encode().unwrap(),
            liar_value.result.encode().unwrap(),
        );
    assert_eq!(reward, 200 + ONE_WIT + ONE_WIT / 2);

    // Create the RadonReport using the reveals and the RADTally script
    let min_consensus = 0.51;
    let active_wips = current_active_wips();
    let clause_result = evaluate_tally_precondition_clause(
        vec![reveal_value.clone(), reveal_value, liar_value],
        min_consensus,
        3,
        &active_wips,
    );
    let script = RADTally {
        filters: vec![RADFilter {
            op: RadonFilters::Mode as u32,
            args: vec![],
        }],
        reducer: RadonReducers::Mode as u32,
    };
    let report = construct_report_from_clause_result(clause_result, &script, 3, &active_wips);
    let report = evaluate_tally_postcondition_clause(report, min_consensus, 3);

    // Create a TallyTransaction using the create_tally function
    let tally_transaction = create_tally(
        dr_pointer,
        &dr_output,
        dr_pkh,
        &report,
        vec![rewarded[0], rewarded[1], dr_pkh],
        vec![rewarded[0], rewarded[1], dr_pkh]
            .iter()
            .cloned()
            .collect::<HashSet<PublicKeyHash>>(),
        ONE_WIT,
        tally_bytes_on_encode_error(),
        &active_wips,
    );

    let x =
        validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips).map(|_| ());
    x.unwrap();
}

#[test]
fn create_tally_validation_5_reveals_1_liar_1_error() {
    let reveal_value = RadonReport::from_result(
        Ok(RadonTypes::from(RadonInteger::from(1))),
        &ReportContext::default(),
    );
    let liar_value = RadonReport::from_result(
        Ok(RadonTypes::from(RadonInteger::from(5))),
        &ReportContext::default(),
    );
    let error_value = RadonReport::from_result(
        Ok(RadonTypes::from(
            RadonError::try_from(RadError::RetrieveTimeout).unwrap(),
        )),
        &ReportContext::default(),
    );

    // Create a DataRequestPool with 5 reveals (one lie and one error)
    let dr_output = example_data_request_output_with_mode_filter(5, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage_with_errors(
            dr_output.clone(),
            5,
            5,
            1,
            1,
            reveal_value.result.encode().unwrap(),
            liar_value.result.encode().unwrap(),
        );
    assert_eq!(reward, 200 + ONE_WIT + ONE_WIT / 3);

    // Create the RadonReport using the reveals and the RADTally script
    let min_consensus = 0.51;
    let active_wips = current_active_wips();
    let clause_result = evaluate_tally_precondition_clause(
        vec![
            reveal_value.clone(),
            reveal_value.clone(),
            reveal_value,
            liar_value,
            error_value,
        ],
        min_consensus,
        5,
        &active_wips,
    );
    let script = RADTally {
        filters: vec![RADFilter {
            op: RadonFilters::Mode as u32,
            args: vec![],
        }],
        reducer: RadonReducers::Mode as u32,
    };
    let report = construct_report_from_clause_result(clause_result, &script, 5, &active_wips);
    let report = evaluate_tally_postcondition_clause(report, min_consensus, 5);

    // Create a TallyTransaction using the create_tally function
    let tally_transaction = create_tally(
        dr_pointer,
        &dr_output,
        dr_pkh,
        &report,
        vec![
            rewarded[0],
            rewarded[1],
            rewarded[2],
            slashed[0],
            error_witnesses[0],
        ],
        vec![
            rewarded[0],
            rewarded[1],
            rewarded[2],
            slashed[0],
            error_witnesses[0],
        ]
        .iter()
        .cloned()
        .collect::<HashSet<PublicKeyHash>>(),
        ONE_WIT,
        tally_bytes_on_encode_error(),
        &active_wips,
    );

    let x =
        validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips).map(|_| ());
    x.unwrap();
}

#[test]
fn create_tally_validation_4_commits_2_reveals() {
    let reveal_value = RadonReport::from_result(
        Ok(RadonTypes::from(RadonInteger::from(1))),
        &ReportContext::default(),
    );

    // Create a DataRequestPool with 4 commits and 2 reveals
    let dr_output = example_data_request_output_with_mode_filter(4, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, _error_witnesses, dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage(
            dr_output.clone(),
            4,
            2,
            0,
            reveal_value.result.encode().unwrap(),
            vec![],
        );
    assert_eq!(reward, 200 + 2 * ONE_WIT);

    // Create the RadonReport using the reveals and the RADTally script
    let min_consensus = 0.51;
    let active_wips = current_active_wips();
    let clause_result = evaluate_tally_precondition_clause(
        vec![reveal_value.clone(), reveal_value],
        min_consensus,
        4,
        &active_wips,
    );
    let script = RADTally {
        filters: vec![RADFilter {
            op: RadonFilters::Mode as u32,
            args: vec![],
        }],
        reducer: RadonReducers::Mode as u32,
    };
    let report = construct_report_from_clause_result(clause_result, &script, 2, &active_wips);
    let report = evaluate_tally_postcondition_clause(report, min_consensus, 4);

    // Create a TallyTransaction using the create_tally function
    let tally_transaction = create_tally(
        dr_pointer,
        &dr_output,
        dr_pkh,
        &report,
        vec![rewarded[0], rewarded[1]],
        vec![rewarded[0], rewarded[1], slashed[0], slashed[1]]
            .iter()
            .cloned()
            .collect::<HashSet<PublicKeyHash>>(),
        ONE_WIT,
        tally_bytes_on_encode_error(),
        &active_wips,
    );

    let x =
        validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips).map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_zero_commits() {
    let dr_output = example_data_request_output(5, 200, 20);
    let (dr_pool, dr_pointer, _rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 0, 0, 0, vec![], vec![]);
    assert_eq!(reward, 0);

    // Tally value: Insufficient commits Error
    let min_consensus = 0.0;
    let active_wips = current_active_wips();
    let clause_result = evaluate_tally_precondition_clause(vec![], min_consensus, 0, &active_wips);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0, &active_wips);
    let report = evaluate_tally_postcondition_clause(report, min_consensus, 0);
    let tally_value = report.result.encode().unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction =
        TallyTransaction::new(dr_pointer, tally_value, vec![vt0], slashed, error_witnesses);
    let x =
        validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips).map(|_| ());
    x.unwrap();
}

#[test]
fn create_tally_validation_zero_commits() {
    let dr_output = example_data_request_output(5, 200, 20);
    let (dr_pool, dr_pointer, _rewarded, _slashed, _error_witnesses, dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output.clone(), 0, 0, 0, vec![], vec![]);
    assert_eq!(reward, 0);

    // Tally value: Insufficient commits Error
    let min_consensus = 0.51;
    let active_wips = current_active_wips();
    let clause_result = evaluate_tally_precondition_clause(vec![], min_consensus, 0, &active_wips);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0, &active_wips);
    let report = evaluate_tally_postcondition_clause(report, min_consensus, 0);
    let tally_transaction = create_tally(
        dr_pointer,
        &dr_output,
        dr_pkh,
        &report,
        vec![],
        HashSet::<PublicKeyHash>::default(),
        ONE_WIT,
        tally_bytes_on_encode_error(),
        &active_wips,
    );
    let x =
        validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips).map(|_| ());
    x.unwrap();
}

#[test]
fn tally_invalid_zero_commits() {
    let dr_output = example_data_request_output(5, 200, 20);
    let (dr_pool, dr_pointer, _rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 0, 0, 0, vec![], vec![]);
    assert_eq!(reward, 0);

    // Tally value: Insufficient commits Error
    let min_consensus = 0.0;
    let active_wips = current_active_wips();
    let clause_result = evaluate_tally_precondition_clause(vec![], min_consensus, 0, &active_wips);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0, &active_wips);
    let report = evaluate_tally_postcondition_clause(report, min_consensus, 0);
    let tally_value = report.result.encode().unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: PublicKeyHash::default(),
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1],
        slashed,
        error_witnesses,
    );
    let x =
        validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips).map(|_| ());
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
    let dr_output = example_data_request_output(5, 200, 20);
    let (dr_pool, dr_pointer, _rewarded, slashed, error_witnesses, dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output.clone(), 5, 0, 0, vec![], vec![]);
    assert_eq!(reward, ONE_WIT);

    // Tally value: NoReveals commits Error
    let min_consensus = 0.51;
    let active_wips = current_active_wips();
    let clause_result = evaluate_tally_precondition_clause(vec![], min_consensus, 5, &active_wips);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0, &active_wips);
    let report = evaluate_tally_postcondition_clause(report, min_consensus, 5);
    let tally_value = report.result.encode().unwrap();

    assert_eq!(reward, dr_output.collateral);
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: slashed[0],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: slashed[1],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: slashed[2],
        value: reward,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: slashed[3],
        value: reward,
    };
    let vt5 = ValueTransferOutput {
        time_lock: 0,
        pkh: slashed[4],
        value: reward,
    };
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt1, vt2, vt3, vt4, vt5, vt0],
        slashed,
        error_witnesses,
    );
    let x =
        validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips).map(|_| ());
    x.unwrap();
}

#[test]
fn create_tally_validation_zero_reveals() {
    let dr_output = example_data_request_output(5, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output.clone(), 5, 0, 0, vec![], vec![]);
    assert_eq!(reward, ONE_WIT);

    // Tally value: NoReveals commits Error
    let min_consensus = 0.51;
    let active_wips = current_active_wips();
    let clause_result = evaluate_tally_precondition_clause(vec![], min_consensus, 5, &active_wips);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0, &active_wips);
    let report = evaluate_tally_postcondition_clause(report, min_consensus, 5);

    let mut committers = rewarded;
    committers.extend(slashed);
    committers.extend(error_witnesses);
    let tally_transaction = create_tally(
        dr_pointer,
        &dr_output,
        dr_pkh,
        &report,
        vec![],
        committers
            .iter()
            .cloned()
            .collect::<HashSet<PublicKeyHash>>(),
        ONE_WIT,
        tally_bytes_on_encode_error(),
        &active_wips,
    );
    let x =
        validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips).map(|_| ());
    x.unwrap();
}

#[test]
fn create_tally_validation_zero_reveals_zero_collateral() {
    let mut dr_output = example_data_request_output(5, 200, 20);
    dr_output.collateral = 0;
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output.clone(), 5, 0, 0, vec![], vec![]);
    assert_eq!(reward, ONE_WIT);

    // Tally value: NoReveals commits Error
    let min_consensus = 0.51;
    let active_wips = current_active_wips();
    let clause_result = evaluate_tally_precondition_clause(vec![], min_consensus, 5, &active_wips);
    let script = RADTally::default();
    let report = construct_report_from_clause_result(clause_result, &script, 0, &active_wips);
    let report = evaluate_tally_postcondition_clause(report, min_consensus, 5);

    let mut committers = rewarded;
    committers.extend(slashed);
    committers.extend(error_witnesses);
    let tally_transaction = create_tally(
        dr_pointer,
        &dr_output,
        dr_pkh,
        &report,
        vec![],
        committers
            .iter()
            .cloned()
            .collect::<HashSet<PublicKeyHash>>(),
        ONE_WIT,
        tally_bytes_on_encode_error(),
        &active_wips,
    );
    let x =
        validate_tally_transaction(&tally_transaction, &dr_pool, ONE_WIT, &active_wips).map(|_| ());
    x.unwrap();
}

#[test]
fn validate_calculate_tally_change() {
    let dr_output = DataRequestOutput {
        witnesses: 5,
        commit_and_reveal_fee: 15,
        witness_reward: 1000,
        ..DataRequestOutput::default()
    };

    // Case 0 commits
    let expected_change = (15 + 15 + 1000) * 5;
    assert_eq!(expected_change, calculate_tally_change(0, 0, 0, &dr_output));

    // Case 0 reveals
    let expected_change = (15 + 1000) * 5;
    assert_eq!(expected_change, calculate_tally_change(5, 0, 0, &dr_output));

    // Case all honests
    let expected_change = 0;
    assert_eq!(expected_change, calculate_tally_change(5, 5, 5, &dr_output));

    // Case 2 liars
    let expected_change = 1000 * 2;
    assert_eq!(
        expected_change,
        calculate_tally_change(5, 5, 5 - 2, &dr_output)
    );

    // Case 1 liar and 1 non-revealer
    let expected_change = 1000 * 2 + 15;
    assert_eq!(
        expected_change,
        calculate_tally_change(5, 4, 4 - 1, &dr_output)
    );
}

#[test]
fn validate_calculate_witness_reward() {
    let dr_output = DataRequestOutput {
        witnesses: 5,
        commit_and_reveal_fee: 15,
        witness_reward: 1000,
        collateral: 5000,
        ..DataRequestOutput::default()
    };

    // Case 0 commits
    let expected_reward = 0;
    let rest = 0;
    assert_eq!(
        (expected_reward, rest),
        calculate_witness_reward(0, 0, 0, dr_output.witness_reward, dr_output.collateral)
    );

    // Case 0 reveals
    let expected_reward = 5000;
    let rest = 0;
    assert_eq!(
        (expected_reward, rest),
        calculate_witness_reward(5, 5, 0, dr_output.witness_reward, dr_output.collateral)
    );

    // Case all honests
    let expected_reward = 1000 + 5000;
    let rest = 0;
    assert_eq!(
        (expected_reward, rest),
        calculate_witness_reward(5, 0, 0, dr_output.witness_reward, dr_output.collateral)
    );

    // Case 2 liars
    let expected_reward = 1000 + 5000 + 5000 * 2 / 3;
    let rest = 5000 * 2 % 3;
    assert_eq!(
        (expected_reward, rest),
        calculate_witness_reward(5, 2, 0, dr_output.witness_reward, dr_output.collateral)
    );

    // Case 1 liar and 1 non-revealer
    let expected_reward = 1000 + 5000 + 5000 * 2 / 3;
    let rest = 5000 * 2 % 3;
    assert_eq!(
        (expected_reward, rest),
        calculate_witness_reward(5, 2, 0, dr_output.witness_reward, dr_output.collateral)
    );

    // Case 1 error
    let expected_reward = 1000 + 5000;
    let rest = 0;
    assert_eq!(
        (expected_reward, rest),
        calculate_witness_reward(5, 0, 1, dr_output.witness_reward, dr_output.collateral)
    );

    // Case 1 liar and 1 error
    let expected_reward = 1000 + 5000 + 5000 / 3;
    let rest = 5000 % 3;
    assert_eq!(
        (expected_reward, rest),
        calculate_witness_reward(5, 1, 1, dr_output.witness_reward, dr_output.collateral)
    );
}

#[test]
fn tally_valid_4_reveals_all_liars() {
    let collateral = ONE_WIT;
    let reveals = vec![vec![24, 60], vec![24, 60], vec![24, 61], vec![24, 47]];
    let stddev_cbor = vec![249, 0, 0];
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(4, reveals, stddev_cbor);
    let change = 1000 * 4;

    // Tally value: insufficient consensus
    let tally_value = vec![
        216, 39, 131, 24, 81, 250, 0, 0, 0, 0, 251, 63, 224, 81, 235, 133, 30, 184, 82,
    ];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_4_reveals_all_liars_attacker_pkh() {
    let collateral = ONE_WIT;
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(
        4,
        vec![vec![24, 60], vec![24, 60], vec![24, 61], vec![24, 47]],
        vec![249, 0, 0],
    );
    let change = 1000 * 4;
    let attacker_pkh = PublicKeyHash::from_bytes(&[0xAA; 20]).unwrap();

    // Tally value: insufficient consensus
    let tally_value = vec![
        216, 39, 131, 24, 81, 250, 0, 0, 0, 0, 251, 63, 224, 81, 235, 133, 30, 184, 82,
    ];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: attacker_pkh,
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    // The attacker_pkh has not participated in the commit/reveal process, so the error is "CommitNotFound"
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::CommitNotFound
    );
}

#[test]
fn tally_valid_4_reveals_2_liars_2_true() {
    let collateral = ONE_WIT;
    let stddev_cbor = vec![249, 0x39, 0]; // 0.625
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(
        4,
        vec![vec![24, 60], vec![24, 60], vec![24, 61], vec![24, 47]],
        stddev_cbor,
    );
    let change = 1000 * 4;

    // Tally value: insufficient consensus
    let tally_value = vec![
        216, 39, 131, 24, 81, 250, 63, 0, 0, 0, 251, 63, 224, 81, 235, 133, 30, 184, 82,
    ];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_4_reveals_2_errors_2_true() {
    let collateral = ONE_WIT;
    let stddev_cbor = vec![249, 0, 0]; // 0.0
    let reveals = vec![
        vec![24, 60],
        vec![24, 60],
        vec![216, 39, 129, 0],
        vec![216, 39, 129, 0],
    ];
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(4, reveals, stddev_cbor);
    let change = 1000 * 4;

    // Tally value: insufficient consensus
    let tally_value = vec![
        216, 39, 131, 24, 81, 250, 63, 0, 0, 0, 251, 63, 224, 81, 235, 133, 30, 184, 82,
    ];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_4_reveals_1_liar_2_true() {
    let collateral = ONE_WIT;
    let stddev_cbor = vec![249, 0x39, 0]; // 0.625
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(
        4,
        vec![vec![24, 60], vec![24, 60], vec![24, 61]],
        stddev_cbor,
    );
    let change = 1000 * 4;

    // Tally value: insufficient consensus
    let tally_value = vec![
        216, 39, 131, 24, 81, 250, 0, 0, 0, 0, 251, 63, 224, 81, 235, 133, 30, 184, 82,
    ];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change + 10,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
        vec![pkhs[0], pkhs[1], pkhs[2]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_4_reveals_invalid_script_arg() {
    let collateral = ONE_WIT;
    // Note: this data request should be impossible to include in a block because it does not pass
    // the data request validation.
    // But it's a useful test for the branch that results in "RadError::TallyExecution".
    // Invalid argument for DeviationStandard filter (invalid CBOR):
    let stddev_cbor = vec![0x3F];
    let reveals = vec![vec![24, 60], vec![24, 60], vec![24, 61], vec![24, 47]];
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(4, reveals, stddev_cbor);
    let change = 1000 * 4;

    // TODO: serialize tally value from RadError to make this test more clear
    let tally_value = vec![
        216, 39, 130, 24, 83, 120, 70, 105, 110, 110, 101, 114, 58, 32, 66, 117, 102, 102, 101,
        114, 73, 115, 78, 111, 116, 86, 97, 108, 117, 101, 32, 123, 32, 100, 101, 115, 99, 114,
        105, 112, 116, 105, 111, 110, 58, 32, 34, 117, 110, 97, 115, 115, 105, 103, 110, 101, 100,
        32, 116, 121, 112, 101, 32, 97, 116, 32, 111, 102, 102, 115, 101, 116, 32, 49, 34, 32, 125,
    ];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_3_reveals_1_no_reveal_invalid_script_arg() {
    let collateral = ONE_WIT;
    // Note: this data request should be impossible to include in a block because it does not pass
    // the data request validation.
    // But it's a useful test for the branch that results in "RadError::TallyExecution".
    // Invalid argument for DeviationStandard filter (invalid CBOR):
    let stddev_cbor = vec![0x3F];
    let reveals = vec![vec![24, 60], vec![24, 60], vec![24, 61]];
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(4, reveals, stddev_cbor);
    let change = 1000 * 4 + 10;

    // TODO: serialize tally value from RadError to make this test more clear
    let tally_value = vec![
        216, 39, 130, 24, 83, 120, 70, 105, 110, 110, 101, 114, 58, 32, 66, 117, 102, 102, 101,
        114, 73, 115, 78, 111, 116, 86, 97, 108, 117, 101, 32, 123, 32, 100, 101, 115, 99, 114,
        105, 112, 116, 105, 111, 110, 58, 32, 34, 117, 110, 97, 115, 115, 105, 103, 110, 101, 100,
        32, 116, 121, 112, 101, 32, 97, 116, 32, 111, 102, 102, 115, 101, 116, 32, 49, 34, 32, 125,
    ];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
        vec![pkhs[0], pkhs[1], pkhs[2]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_4_reveals_majority_of_errors() {
    let collateral = ONE_WIT;
    let stddev_cbor = vec![249, 0, 0]; // 0.0
                                       // RetrieveTimeout
    let reveals = vec![
        vec![216, 39, 129, 24, 49],
        vec![216, 39, 129, 24, 49],
        vec![216, 39, 129, 24, 49],
        vec![216, 39, 129, 24, 49],
    ];
    let (pkhs, _dr_pkh, dr_pointer, dr_pool) =
        generic_tally_test_stddev_dr(4, reveals, stddev_cbor);
    let reward = collateral + 1000;

    // TODO: serialize tally value from RadError to make this test more clear
    let tally_value = vec![216, 39, 129, 24, 49];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: reward,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3],
        vec![],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_3_reveals_1_no_reveal_majority_of_errors() {
    let collateral = ONE_WIT;
    let stddev_cbor = vec![249, 0, 0]; // 0.0
                                       // RetrieveTimeout
    let reveals = vec![
        vec![216, 39, 129, 24, 49],
        vec![216, 39, 129, 24, 49],
        vec![216, 39, 129, 24, 49],
    ];
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(4, reveals, stddev_cbor);
    let reward = collateral + 1000 + collateral / 3;
    let change = 1000 + 10;

    // TODO: serialize tally value from RadError to make this test more clear
    let tally_value = vec![216, 39, 129, 24, 49];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3],
        vec![pkhs[3]],
        vec![pkhs[0], pkhs[1], pkhs[2]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_2_reveals_2_no_reveals_majority_of_errors_insufficient_consensus() {
    let collateral = ONE_WIT;
    let stddev_cbor = vec![249, 0, 0]; // 0.0
                                       // RetrieveTimeout
    let reveals = vec![vec![216, 39, 129, 24, 49], vec![216, 39, 129, 24, 49]];
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(4, reveals, stddev_cbor);
    let change = 1000 * 4 + 10 * 2;

    // TODO: serialize tally value from RadError to make this test more clear
    let tally_value = vec![
        216, 39, 131, 24, 81, 250, 63, 0, 0, 0, 251, 63, 224, 81, 235, 133, 30, 184, 82,
    ];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
        vec![pkhs[0], pkhs[1]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_4_reveals_majority_of_errors_insufficient_consensus() {
    let collateral = ONE_WIT;
    let stddev_cbor = vec![249, 0, 0]; // 0.0

    // There is only 50% consensus for error RetrieveTimeout
    let reveals = vec![
        vec![216, 39, 129, 24, 49],
        vec![216, 39, 129, 24, 49],
        vec![216, 39, 129, 24, 64], // Overflow
        vec![216, 39, 129, 24, 65], // Underflow
    ];
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(4, reveals, stddev_cbor);
    let change = 1000 * 4;

    // TODO: serialize tally value from RadError to make this test more clear
    let tally_value = vec![
        216, 39, 131, 24, 81, 250, 63, 0, 0, 0, 251, 63, 224, 81, 235, 133, 30, 184, 82,
    ];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_3_reveals_1_no_reveal_majority_of_errors_insufficient_consensus() {
    let collateral = ONE_WIT;
    let stddev_cbor = vec![249, 0, 0]; // 0.0

    // There is only 50% consensus for error RetrieveTimeout
    let reveals = vec![
        vec![216, 39, 129, 24, 49],
        vec![216, 39, 129, 24, 49],
        vec![216, 39, 129, 24, 64], // Overflow
    ];
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_stddev_dr(4, reveals, stddev_cbor);
    let change = 1000 * 4 + 10;

    // TODO: serialize tally value from RadError to make this test more clear
    let tally_value = vec![
        216, 39, 131, 24, 81, 250, 63, 0, 0, 0, 251, 63, 224, 81, 235, 133, 30, 184, 82,
    ];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: collateral,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: collateral,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: collateral,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
        vec![pkhs[0], pkhs[1], pkhs[2]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_rng() {
    let reveals = vec![
        RadonTypes::from(RadonBytes::from(
            hex::decode("6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b")
                .unwrap(),
        )),
        RadonTypes::from(RadonBytes::from(
            hex::decode("d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35")
                .unwrap(),
        )),
        RadonTypes::from(RadonBytes::from(
            hex::decode("4e07408562bedb8b60ce05c1decfe3ad16b72230967de01f640b7e4729b49fce")
                .unwrap(),
        )),
        RadonTypes::from(RadonBytes::from(
            hex::decode("4b227777d4dd1fc61c6f884f48641d02b4d121d3fd328cb08b5531fcacdabf8a")
                .unwrap(),
        )),
    ];
    let reveals = reveals.into_iter().map(|x| x.encode().unwrap()).collect();
    let (pkhs, _dr_pkh, dr_pointer, dr_pool) = generic_tally_test_rng(4, reveals);
    let collateral = ONE_WIT;
    let reward = collateral + 200;

    let tally_value = RadonTypes::from(RadonBytes::from(
        hex::decode("0eb583894535900b2b3b71285f242ee4dda6681b38c802e1a71defe52372e0d4").unwrap(),
    ))
    .encode()
    .unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: reward,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3],
        vec![],
        vec![],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_rng_wrong_bytes_len() {
    let reveals = vec![
        RadonTypes::from(RadonBytes::from(hex::decode("").unwrap())),
        RadonTypes::from(RadonBytes::from(hex::decode("d4").unwrap())),
        RadonTypes::from(RadonBytes::from(hex::decode("4e07408562bedb8b60ce05c1decfe3ad16b72230967de01f640b7e4729b49fce").unwrap())),
        RadonTypes::from(RadonBytes::from(hex::decode("4b227777d4dd1fc61c6f884f48641d02b4d121d3fd328cb08b5531fcacdabf8affffffffffffffffffffffff").unwrap())),
    ];
    let reveals = reveals.into_iter().map(|x| x.encode().unwrap()).collect();
    let (pkhs, _dr_pkh, dr_pointer, dr_pool) = generic_tally_test_rng(4, reveals);
    let collateral = ONE_WIT;
    let reward = collateral + 200;

    let tally_value = RadonTypes::from(RadonBytes::from(
        hex::decode("1ce0d369c43108958b266e481f03b636985eb1808b40da363666b3b2368a46fc").unwrap(),
    ))
    .encode()
    .unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: reward,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3],
        vec![],
        vec![],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_rng_one_error() {
    let reveals = vec![
        RadonTypes::from(RadonBytes::from(
            hex::decode("6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b")
                .unwrap(),
        )),
        RadonTypes::from(RadonBytes::from(
            hex::decode("d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35")
                .unwrap(),
        )),
        RadonTypes::from(RadonBytes::from(
            hex::decode("4e07408562bedb8b60ce05c1decfe3ad16b72230967de01f640b7e4729b49fce")
                .unwrap(),
        )),
        RadonTypes::from(
            RadonError::try_from(RadError::TallyExecution {
                inner: None,
                message: Some("dummy error".to_string()),
            })
            .unwrap(),
        ),
    ];
    let reveals = reveals.into_iter().map(|x| x.encode().unwrap()).collect();
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_rng(4, reveals);
    let collateral = ONE_WIT;
    let reward = collateral + 200;
    let change = 200;

    let tally_value = RadonTypes::from(RadonBytes::from(
        hex::decode("ddf68feb872a32980477d818e57fbfce2d3d7711eb8d2b4638b2e27e215df031").unwrap(),
    ))
    .encode()
    .unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: collateral,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3, vt4],
        vec![pkhs[3]],
        vec![pkhs[3]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_rng_all_errors() {
    let reveals = vec![
        RadonTypes::from(
            RadonError::try_from(RadError::TallyExecution {
                inner: None,
                message: Some("dummy error".to_string()),
            })
            .unwrap(),
        ),
        RadonTypes::from(
            RadonError::try_from(RadError::TallyExecution {
                inner: None,
                message: Some("dummy error".to_string()),
            })
            .unwrap(),
        ),
        RadonTypes::from(
            RadonError::try_from(RadError::TallyExecution {
                inner: None,
                message: Some("dummy error".to_string()),
            })
            .unwrap(),
        ),
        RadonTypes::from(
            RadonError::try_from(RadError::TallyExecution {
                inner: None,
                message: Some("dummy error".to_string()),
            })
            .unwrap(),
        ),
    ];
    let reveals = reveals.into_iter().map(|x| x.encode().unwrap()).collect();
    let (pkhs, _dr_pkh, dr_pointer, dr_pool) = generic_tally_test_rng(4, reveals);
    let collateral = ONE_WIT;
    let reward = collateral + 200;

    let tally_value = RadonTypes::from(
        RadonError::try_from(RadError::TallyExecution {
            inner: None,
            message: Some("dummy error".to_string()),
        })
        .unwrap(),
    )
    .encode()
    .unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: reward,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3],
        vec![],
        vec![pkhs[0], pkhs[1], pkhs[2], pkhs[3]],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_rng_one_invalid_type() {
    let reveals = vec![
        RadonTypes::from(RadonBytes::from(
            hex::decode("6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b")
                .unwrap(),
        )),
        RadonTypes::from(RadonBytes::from(
            hex::decode("d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35")
                .unwrap(),
        )),
        RadonTypes::from(RadonBytes::from(
            hex::decode("4e07408562bedb8b60ce05c1decfe3ad16b72230967de01f640b7e4729b49fce")
                .unwrap(),
        )),
        RadonTypes::from(RadonInteger::from(4)),
    ];
    let reveals = reveals.into_iter().map(|x| x.encode().unwrap()).collect();
    let (pkhs, dr_pkh, dr_pointer, dr_pool) = generic_tally_test_rng(4, reveals);
    let collateral = ONE_WIT;
    let reward = collateral + 200 + collateral / 3;
    let change = 200;

    let tally_value = RadonTypes::from(RadonBytes::from(
        hex::decode("ddf68feb872a32980477d818e57fbfce2d3d7711eb8d2b4638b2e27e215df031").unwrap(),
    ))
    .encode()
    .unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: reward,
    };
    let vt4 = ValueTransferOutput {
        time_lock: 0,
        pkh: dr_pkh,
        value: change,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt4],
        vec![pkhs[3]],
        vec![],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_valid_rng_all_invalid_type() {
    let reveals = vec![
        RadonTypes::from(RadonInteger::from(1)),
        RadonTypes::from(RadonInteger::from(2)),
        RadonTypes::from(RadonInteger::from(3)),
        RadonTypes::from(RadonInteger::from(4)),
    ];
    let reveals = reveals.into_iter().map(|x| x.encode().unwrap()).collect();
    let (pkhs, _dr_pkh, dr_pointer, dr_pool) = generic_tally_test_rng(4, reveals);
    let collateral = ONE_WIT;
    let reward = collateral + 200;

    let tally_value = RadonTypes::from(
        RadonError::try_from(RadError::UnhandledInterceptV2 { inner: None }).unwrap(),
    )
    .encode()
    .unwrap();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[1],
        value: reward,
    };
    let vt2 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[2],
        value: reward,
    };
    let vt3 = ValueTransferOutput {
        time_lock: 0,
        pkh: pkhs[3],
        value: reward,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1, vt2, vt3],
        vec![],
        vec![],
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_bytes_on_encode_error_does_not_change() {
    let bytes = tally_bytes_on_encode_error();
    let expected = vec![216, 39, 129, 0];

    assert_eq!(bytes, expected);
}

#[test]
fn tally_unserializable_value() {
    // When the result of the tally cannot be serialized, the result is a RadError::Unknown, as
    // returned by `tally_bytes_on_encode_error`

    // Reveal value: negative(18446744073709551361)
    let reveal_value = vec![59, 255, 255, 255, 255, 255, 255, 255, 0];
    let dr_output = example_data_request_output_average_mean_reducer(2, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, _dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 2, 2, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT);

    // Tally value: RadError::Unknown
    let tally_value = tally_bytes_on_encode_error();
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    assert_eq!(change, 0);
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt0, vt1],
        slashed,
        error_witnesses,
    );
    let x = validate_tally_transaction(
        &tally_transaction,
        &dr_pool,
        ONE_WIT,
        &current_active_wips(),
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_unhandled_intercept_with_message() {
    // Reveals with value RadonErrors::UnhandledIntercept are accepted, but the message field must
    // be removed from the tally transaction.

    // Reveal value: 39([255, "Hello!"])
    let reveal_value = vec![
        216, 39, 130, 24, 255, 0x66, b'H', b'e', b'l', b'l', b'o', b'!',
    ];
    let dr_output = example_data_request_output(2, 200, 20);
    let (dr_pool, dr_pointer, _rewarded, slashed, error_witnesses, _dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_all_errors(dr_output, 2, 2, reveal_value.clone());
    assert_eq!(reward, ONE_WIT + 200);

    // Tally value with message: 39([255, "Hello!"])
    let tally_value_with_message = reveal_value;
    // Tally value no message: 39([255])
    let tally_value_no_message = vec![216, 39, 129, 24, 255];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: error_witnesses[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: error_witnesses[1],
        value: reward,
    };
    assert_eq!(change, 0);
    let tally_transaction_with_message = TallyTransaction::new(
        dr_pointer,
        tally_value_with_message.clone(),
        vec![vt0.clone(), vt1.clone()],
        slashed.clone(),
        error_witnesses.clone(),
    );
    let tally_transaction_no_message = TallyTransaction::new(
        dr_pointer,
        tally_value_no_message.clone(),
        vec![vt0, vt1],
        slashed,
        error_witnesses,
    );

    let mut active_wips = current_active_wips();
    // Disable WIP-0018
    active_wips.active_wips.remove("WIP0017-0018-0019");

    // Before WIP-0018:
    // tally_transaction_with_message is valid, tally_transaction_no_message is invalid
    let x = validate_tally_transaction(
        &tally_transaction_with_message,
        &dr_pool,
        ONE_WIT,
        &active_wips,
    )
    .map(|_| ());
    x.unwrap();
    let x = validate_tally_transaction(
        &tally_transaction_no_message,
        &dr_pool,
        ONE_WIT,
        &active_wips,
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchedConsensus {
            miner_tally: tally_value_no_message.clone(),
            expected_tally: tally_value_with_message.clone(),
        }
    );

    // Enable WIP-0018
    active_wips
        .active_wips
        .insert("WIP0017-0018-0019".to_string(), 0);

    // After WIP-0018:
    // tally_transaction_with_message is invalid, tally_transaction_no_message is valid
    let x = validate_tally_transaction(
        &tally_transaction_with_message,
        &dr_pool,
        ONE_WIT,
        &active_wips,
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchedConsensus {
            miner_tally: tally_value_with_message,
            expected_tally: tally_value_no_message,
        }
    );
    let x = validate_tally_transaction(
        &tally_transaction_no_message,
        &dr_pool,
        ONE_WIT,
        &active_wips,
    )
    .map(|_| ());
    x.unwrap();
}

#[test]
fn tally_unhandled_intercept_mode_tie_has_no_message() {
    // Check that UnhandledIntercept errors created during tally execution are serialized without
    // the message field if WIP0018 is enabled

    // Reveal value: integer(1)
    let reveal_value_1 = vec![0x01];
    // Reveal value: integer(2)
    let reveal_value_2 = vec![0x02];
    let reveal_values = vec![reveal_value_1, reveal_value_2];
    let dr_output = example_data_request_output(2, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, _dr_pkh, change, reward) =
        dr_pool_with_dr_in_tally_stage_different_reveals(dr_output, 2, reveal_values, vec![]);
    assert_eq!(reward, ONE_WIT + 200);

    // Expeted tally value: ModeTie error because the mode of [1, 2] is not defined
    // Tally value with message: 39([255, "(long description of ModeTie error)"])
    let tally_value_with_message = vec![
        216, 39, 130, 24, 255, 120, 157, 105, 110, 110, 101, 114, 58, 32, 77, 111, 100, 101, 84,
        105, 101, 32, 123, 32, 118, 97, 108, 117, 101, 115, 58, 32, 82, 97, 100, 111, 110, 65, 114,
        114, 97, 121, 32, 123, 32, 118, 97, 108, 117, 101, 58, 32, 91, 73, 110, 116, 101, 103, 101,
        114, 40, 82, 97, 100, 111, 110, 73, 110, 116, 101, 103, 101, 114, 32, 123, 32, 118, 97,
        108, 117, 101, 58, 32, 50, 32, 125, 41, 44, 32, 73, 110, 116, 101, 103, 101, 114, 40, 82,
        97, 100, 111, 110, 73, 110, 116, 101, 103, 101, 114, 32, 123, 32, 118, 97, 108, 117, 101,
        58, 32, 49, 32, 125, 41, 93, 44, 32, 105, 115, 95, 104, 111, 109, 111, 103, 101, 110, 101,
        111, 117, 115, 58, 32, 116, 114, 117, 101, 32, 125, 44, 32, 109, 97, 120, 95, 99, 111, 117,
        110, 116, 58, 32, 49, 32, 125,
    ];
    // Tally value no message: 39([255])
    let tally_value_no_message = vec![216, 39, 129, 24, 255];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    assert_eq!(change, 0);
    let tally_transaction_with_message = TallyTransaction::new(
        dr_pointer,
        tally_value_with_message.clone(),
        vec![vt0.clone(), vt1.clone()],
        slashed.clone(),
        error_witnesses.clone(),
    );
    let tally_transaction_no_message = TallyTransaction::new(
        dr_pointer,
        tally_value_no_message.clone(),
        vec![vt0, vt1],
        slashed,
        error_witnesses,
    );

    let mut active_wips = current_active_wips();
    // Disable WIP-0018
    active_wips.active_wips.remove("WIP0017-0018-0019");

    // Before WIP-0018:
    // tally_transaction_with_message is valid, tally_transaction_no_message is invalid
    let x = validate_tally_transaction(
        &tally_transaction_with_message,
        &dr_pool,
        ONE_WIT,
        &active_wips,
    )
    .map(|_| ());
    x.unwrap();
    let x = validate_tally_transaction(
        &tally_transaction_no_message,
        &dr_pool,
        ONE_WIT,
        &active_wips,
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchedConsensus {
            miner_tally: tally_value_no_message.clone(),
            expected_tally: tally_value_with_message.clone(),
        }
    );

    // Enable WIP-0018
    active_wips
        .active_wips
        .insert("WIP0017-0018-0019".to_string(), 0);

    // After WIP-0018:
    // tally_transaction_with_message is invalid, tally_transaction_no_message is valid
    let x = validate_tally_transaction(
        &tally_transaction_with_message,
        &dr_pool,
        ONE_WIT,
        &active_wips,
    )
    .map(|_| ());
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::MismatchedConsensus {
            miner_tally: tally_value_with_message,
            expected_tally: tally_value_no_message,
        }
    );
    let x = validate_tally_transaction(
        &tally_transaction_no_message,
        &dr_pool,
        ONE_WIT,
        &active_wips,
    )
    .map(|_| ());
    x.unwrap();
}

static LAST_VRF_INPUT: &str = "4da71b67e7e50ae4ad06a71e505244f8b490da55fc58c50386c908f7146d2239";

#[test]
fn block_signatures() {
    let mut b = Block::new(Default::default(), Default::default(), Default::default());
    // Add valid vrf proof
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let secret_key = SecretKey {
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
    };

    let last_vrf_input = LAST_VRF_INPUT.parse().unwrap();
    let vrf_input = CheckpointVRF {
        hash_prev_vrf: last_vrf_input,
        checkpoint: 0,
    };

    b.block_header.proof = BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap();

    let hashable = b;
    let f = |mut b: Block, ks| -> Result<_, failure::Error> {
        b.block_sig = ks;
        let mut signatures_to_verify = vec![];
        validate_block_signature(&b, &mut signatures_to_verify)?;
        verify_signatures_test(signatures_to_verify)?;
        Ok(())
    };

    let ks = sign_tx(PRIV_KEY_1, &hashable);
    let hash = hashable.hash();

    // Replace the signature with default (all zeros)
    let ks_default = KeyedSignature::default();
    let signature_pkh = ks_default.public_key.pkh();
    let x = f(hashable.clone(), ks_default);
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::PublicKeyHashMismatch {
            proof_pkh: MY_PKH_1.parse().unwrap(),
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
            proof_pkh: MY_PKH_1.parse().unwrap(),
            signature_pkh,
        }
    );

    // Sign transaction with a different public key
    let ks_different_pk = sign_tx(PRIV_KEY_2, &hashable);
    let signature_pkh = ks_different_pk.public_key.pkh();
    let x = f(hashable, ks_different_pk);
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::PublicKeyHashMismatch {
            proof_pkh: MY_PKH_1.parse().unwrap(),
            signature_pkh,
        }
    );
}

static MILLION_TX_OUTPUT: &str =
    "0f0f000000000000000000000000000000000000000000000000000000000000:0";
static MILLION_TX_OUTPUT2: &str =
    "0f0f000000000000000000000000000000000000000000000000000000000001:0";

static BOOTSTRAP_HASH: &str = "4404291750b0cff95068e9894040e84e27cfdab1cb14f8c59928c3480a155b68";
static GENESIS_BLOCK_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";
static LAST_BLOCK_HASH: &str = "62adde3e36db3f22774cc255215b2833575f66bf2204011f80c03d34c7c9ea41";

fn test_block<F: FnMut(&mut Block) -> bool>(mut_block: F) -> Result<(), failure::Error> {
    test_block_with_drpool(mut_block, DataRequestPool::default())
}

fn test_block_with_epoch<F: FnMut(&mut Block) -> bool>(
    mut_block: F,
    epoch: Epoch,
) -> Result<(), failure::Error> {
    test_block_with_drpool_and_utxo_set(
        mut_block,
        DataRequestPool::default(),
        UnspentOutputsPool::default(),
        epoch,
    )
}

fn test_block_with_drpool<F: FnMut(&mut Block) -> bool>(
    mut_block: F,
    dr_pool: DataRequestPool,
) -> Result<(), failure::Error> {
    test_block_with_drpool_and_utxo_set(mut_block, dr_pool, UnspentOutputsPool::default(), E)
}

fn test_block_with_drpool_and_utxo_set<F: FnMut(&mut Block) -> bool>(
    mut mut_block: F,
    dr_pool: DataRequestPool,
    mut utxo_set: UnspentOutputsPool,
    current_epoch: u32,
) -> Result<(), failure::Error> {
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let block_number = 100_000;

    let consensus_constants = ConsensusConstants {
        checkpoint_zero_timestamp: 0,
        collateral_minimum: 1,
        bootstrapping_committee: vec![],
        collateral_age: 1,
        superblock_period: 0,
        mining_backup_factor: 8,
        bootstrap_hash: BOOTSTRAP_HASH.parse().unwrap(),
        genesis_hash: GENESIS_BLOCK_HASH.parse().unwrap(),
        max_dr_weight: MAX_DR_WEIGHT,
        activity_period: 0,
        reputation_expire_alpha_diff: 0,
        reputation_issuance: 0,
        reputation_issuance_stop: 0,
        max_vt_weight: MAX_VT_WEIGHT,
        checkpoints_period: 0,
        reputation_penalization_factor: 0.0,
        mining_replication_factor: 0,
        extra_rounds: 0,
        minimum_difficulty: 2,
        epochs_with_minimum_difficulty: 0,
        superblock_signing_committee_size: 100,
        superblock_committee_decreasing_period: 100,
        superblock_committee_decreasing_step: 5,
        initial_block_reward: INITIAL_BLOCK_REWARD,
        halving_period: HALVING_PERIOD,
    };
    // TODO: In this test the active wips depend on the current epoch
    // Ideally this should use all_wips_active() so that when adding new WIPs the existing tests
    // will fail if the logic is accidentally changed.
    let active_wips = active_wips_from_mainnet(current_epoch);

    // Insert output to utxo
    let output1 = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1_000_000,
    };
    //let tx_output1 = VTTransactionBody::new(vec![], vec![output1.clone()]);
    //let output1_pointer = OutputPointer { transaction_id: tx_output1.hash(), output_index: 0 };
    let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
    utxo_set.insert(output1_pointer, output1, 0);

    let secret_key = SecretKey {
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
    };
    let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();
    let last_vrf_input = LAST_VRF_INPUT.parse().unwrap();
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let block_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };

    let vrf_input = CheckpointVRF {
        checkpoint: current_epoch,
        hash_prev_vrf: last_vrf_input,
    };

    let my_pkh = PublicKeyHash::default();

    let txns = BlockTransactions {
        mint: MintTransaction::new(
            current_epoch,
            vec![ValueTransferOutput {
                time_lock: 0,
                pkh: my_pkh,
                value: block_reward(current_epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD),
            }],
        ),
        ..BlockTransactions::default()
    };

    let block_header = BlockHeader {
        merkle_roots: BlockMerkleRoots::from_transactions(&txns),
        beacon: block_beacon,
        proof: BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap(),
        ..Default::default()
    };
    let block_sig = sign_tx(PRIV_KEY_1, &block_header);
    let mut b = Block::new(block_header, block_sig, txns);

    // Pass the block to the mutation function used by tests
    if mut_block(&mut b) {
        // If the function returns true, re-sign the block after mutating
        b.block_sig = sign_tx(PRIV_KEY_1, &b.block_header);
    }
    let mut signatures_to_verify = vec![];
    validate_block(
        &b,
        current_epoch,
        vrf_input,
        chain_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        &consensus_constants,
        &active_wips,
    )?;
    verify_signatures_test(signatures_to_verify)?;
    let mut signatures_to_verify = vec![];

    validate_block_transactions(
        &utxo_set,
        &dr_pool,
        &b,
        vrf_input,
        &mut signatures_to_verify,
        &rep_eng,
        EpochConstants::default(),
        block_number,
        &consensus_constants,
        &active_wips,
    )?;
    verify_signatures_test(signatures_to_verify)?;

    Ok(())
}

///////////////////////////////////////////////////////////////////////////////
// Block tests: one block
///////////////////////////////////////////////////////////////////////////////

#[test]
fn block_from_the_future() {
    let current_epoch = 1000;
    let block_epoch = current_epoch + 1;

    let x = test_block_with_epoch(
        |b| {
            assert_eq!(current_epoch, b.block_header.beacon.checkpoint);
            b.block_header.beacon.checkpoint = block_epoch;

            true
        },
        current_epoch,
    );
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::BlockFromFuture {
            current_epoch,
            block_epoch
        }
    );
}

#[test]
fn block_from_the_past() {
    let current_epoch = 1000;
    let block_epoch = current_epoch - 1;

    let x = test_block_with_epoch(
        |b| {
            assert_eq!(current_epoch, b.block_header.beacon.checkpoint);
            b.block_header.beacon.checkpoint = block_epoch;

            true
        },
        current_epoch,
    );
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::BlockOlderThanTip {
            chain_epoch: current_epoch,
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

    let last_vrf_input = LAST_VRF_INPUT.parse().unwrap();

    let vrf_input = CheckpointVRF {
        checkpoint: 1000,
        hash_prev_vrf: last_vrf_input,
    };

    let x = test_block(|b| {
        assert_ne!(genesis_hash, b.block_header.beacon.hash_prev_block);
        b.block_header.beacon.hash_prev_block = genesis_hash;

        // Re-create a valid VRF proof
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey {
            bytes: Protected::from(PRIV_KEY_1.to_vec()),
        };

        b.block_header.proof = BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap();

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
fn block_signals_can_be_anything() {
    // The signals field in the block header can have any value, the block will always be valid
    // Test some arbitrary values of "signals"
    for signals in &[0, 1, 2, 3, 255, 256, u32::MAX - 1, u32::MAX] {
        if let Err(e) = test_block(|b| {
            b.block_header.signals = *signals;

            true
        }) {
            panic!("Failed to validate block with signals {}: {:?}", signals, e);
        }
    }
}

#[test]
fn block_invalid_poe() {
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let secret_key = SecretKey {
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
    };
    let block_elegibility_claim =
        BlockEligibilityClaim::create(vrf, &secret_key, CheckpointVRF::default()).unwrap();
    let x = test_block(|b| {
        b.block_header.proof = block_elegibility_claim.clone();

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
        .ars_mut()
        .push_activity((0..512).map(|x| PublicKeyHash::from_hex(&format!("{:040}", x)).unwrap()));
    let mut utxo_set = UnspentOutputsPool::default();
    let block_number = 0;

    let consensus_constants = ConsensusConstants {
        checkpoint_zero_timestamp: 0,
        collateral_minimum: 1,
        bootstrapping_committee: vec![],
        collateral_age: 1,
        superblock_period: 0,
        mining_backup_factor: 8,
        bootstrap_hash: BOOTSTRAP_HASH.parse().unwrap(),
        genesis_hash: GENESIS_BLOCK_HASH.parse().unwrap(),
        max_dr_weight: MAX_DR_WEIGHT,
        activity_period: 0,
        reputation_expire_alpha_diff: 0,
        reputation_issuance: 0,
        reputation_issuance_stop: 0,
        max_vt_weight: MAX_VT_WEIGHT,
        checkpoints_period: 0,
        reputation_penalization_factor: 0.0,
        mining_replication_factor: 0,
        extra_rounds: 0,
        minimum_difficulty: 0,
        epochs_with_minimum_difficulty: 0,
        superblock_signing_committee_size: 100,
        superblock_committee_decreasing_period: 100,
        superblock_committee_decreasing_step: 5,
        initial_block_reward: INITIAL_BLOCK_REWARD,
        halving_period: HALVING_PERIOD,
    };

    // Insert output to utxo
    let output1 = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1_000_000,
    };
    //let tx_output1 = VTTransactionBody::new(vec![], vec![output1.clone()]);
    //let output1_pointer = OutputPointer { transaction_id: tx_output1.hash(), output_index: 0 };
    let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
    utxo_set.insert(output1_pointer, output1, block_number);

    let secret_key = SecretKey {
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
    };
    let current_epoch = 1000;
    let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();
    let last_vrf_input = LAST_VRF_INPUT.parse().unwrap();

    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };

    let vrf_input = CheckpointVRF {
        checkpoint: current_epoch,
        hash_prev_vrf: last_vrf_input,
    };

    let block_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };
    let my_pkh = PublicKeyHash::default();

    let txns = BlockTransactions {
        mint: MintTransaction::new(
            current_epoch,
            vec![ValueTransferOutput {
                time_lock: 0,
                pkh: my_pkh,
                value: block_reward(current_epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD),
            }],
        ),
        ..BlockTransactions::default()
    };

    let block_header = BlockHeader {
        merkle_roots: BlockMerkleRoots::from_transactions(&txns),
        beacon: block_beacon,
        proof: BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap(),
        ..Default::default()
    };
    let block_sig = sign_tx(PRIV_KEY_1, &block_header);
    let b = Block::new(block_header, block_sig, txns);

    let x = {
        let x = || -> Result<_, failure::Error> {
            let mut signatures_to_verify = vec![];

            validate_block(
                &b,
                current_epoch,
                vrf_input,
                chain_beacon,
                &mut signatures_to_verify,
                &rep_eng,
                &consensus_constants,
                &current_active_wips(),
            )?;
            verify_signatures_test(signatures_to_verify)?;
            let mut signatures_to_verify = vec![];

            validate_block_transactions(
                &utxo_set,
                &dr_pool,
                &b,
                vrf_input,
                &mut signatures_to_verify,
                &rep_eng,
                EpochConstants::default(),
                block_number,
                &consensus_constants,
                &current_active_wips(),
            )?;
            verify_signatures_test(signatures_to_verify)?;

            Ok(())
        };

        x()
    };

    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::BlockEligibilityDoesNotMeetTarget {
            vrf_hash: "a6e9f38e1115d940b735b391c401a019554da6c7bac2ca022c00f1718892aacf"
                .parse()
                .unwrap(),
            target_hash: Hash::with_first_u32(0x03ff_ffff),
        },
    );
}

#[test]
fn block_change_mint() {
    let x = test_block(|b| {
        assert_ne!(b.txns.mint.outputs[0].pkh, MY_PKH_1.parse().unwrap());
        b.txns.mint = MintTransaction::new(
            b.txns.mint.epoch,
            vec![ValueTransferOutput {
                time_lock: 0,
                pkh: MY_PKH_1.parse().unwrap(),
                ..b.txns.mint.outputs[0]
            }],
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
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);
        b.txns.value_transfer_txns.push(vt_tx);

        old_mint_value = Some(transaction_outputs_sum(&b.txns.mint.outputs).unwrap());

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
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);
        b.txns.value_transfer_txns.push(vt_tx);

        b.txns.mint = MintTransaction::new(
            b.txns.mint.epoch,
            vec![ValueTransferOutput {
                time_lock: 0,
                value: transaction_outputs_sum(&b.txns.mint.outputs).unwrap() + 1_000_000 - 1,
                ..b.txns.mint.outputs[0]
            }],
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
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
    };
    let current_epoch = 1000;
    let last_vrf_input = LAST_VRF_INPUT.parse().unwrap();

    let vrf_input = CheckpointVRF {
        checkpoint: current_epoch,
        hash_prev_vrf: last_vrf_input,
    };

    let dro = DataRequestOutput {
        witness_reward: 1000 / 2,
        commit_and_reveal_fee: 50,
        witnesses: 2,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    let dr_epoch = 0;
    dr_pool
        .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
        .unwrap();

    // Insert valid proof
    let mut cb = CommitTransactionBody::default();
    cb.dr_pointer = dr_hash;
    cb.proof = DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, dr_hash).unwrap();

    let vto1 = ValueTransferOutput {
        pkh: cb.proof.proof.pkh(),
        value: ONE_WIT,
        time_lock: 0,
    };
    let vto2 = ValueTransferOutput {
        pkh: cb.proof.proof.pkh(),
        value: ONE_WIT,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto1, vto2], None, vec![]);
    let vti1 = Input::new(utxo_set.iter().next().unwrap().0);
    let vti2 = Input::new(utxo_set.iter().nth(1).unwrap().0);

    cb.collateral = vec![vti1];
    cb.outputs = vec![];

    // Sign commitment
    let cs = sign_tx(PRIV_KEY_1, &cb);
    let c_tx = CommitTransaction::new(cb.clone(), vec![cs]);

    let mut cb2 = CommitTransactionBody::default();
    cb2.dr_pointer = cb.dr_pointer;
    cb2.proof = cb.proof;
    cb2.commitment = Hash::SHA256([1; 32]);
    cb2.collateral = vec![vti2];
    cb2.outputs = vec![];
    let cs2 = sign_tx(PRIV_KEY_1, &cb2);
    let c2_tx = CommitTransaction::new(cb2, vec![cs2]);

    assert_ne!(c_tx.hash(), c2_tx.hash());

    let x = test_block_with_drpool_and_utxo_set(
        |b| {
            // We include two commits with same pkh and dr_pointer
            b.txns.commit_txns.push(c_tx.clone());
            b.txns.commit_txns.push(c2_tx.clone());

            b.txns.mint = MintTransaction::new(
                b.txns.mint.epoch,
                vec![ValueTransferOutput {
                    time_lock: 0,
                    value: transaction_outputs_sum(&b.txns.mint.outputs).unwrap() + 100, // commit_and_reveal_fee is 50*2
                    ..b.txns.mint.outputs[0]
                }],
            );

            b.block_header.merkle_roots = BlockMerkleRoots::from_transactions(&b.txns);

            true
        },
        dr_pool,
        utxo_set,
        E,
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
        commit_and_reveal_fee: 50,
        min_consensus_percentage: 51,
        data_request: example_data_request(),
        collateral: ONE_WIT,
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro);
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_hash = dr_transaction.hash();
    let dr_epoch = 0;
    dr_pool
        .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
        .unwrap();

    // Hack: get public key by signing an empty transaction
    let public_key = sign_tx(PRIV_KEY_1, &RevealTransactionBody::default()).public_key;
    let public_key2 = sign_tx(PRIV_KEY_2, &RevealTransactionBody::default()).public_key;

    let dr_pointer = dr_hash;

    // Create Reveal and Commit
    // Reveal = empty array
    let reveal_value = vec![0x00];
    let reveal_body =
        RevealTransactionBody::new(dr_pointer, reveal_value.clone(), public_key.pkh());
    let reveal_signature = sign_tx(PRIV_KEY_1, &reveal_body);
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
    let reveal_signature2 = sign_tx(PRIV_KEY_2, &reveal_body2);
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
                vec![ValueTransferOutput {
                    time_lock: 0,
                    value: transaction_outputs_sum(&b.txns.mint.outputs).unwrap() + 100, // commit_and_reveal_fee is 50*2
                    ..b.txns.mint.outputs[0]
                }],
            );

            b.block_header.merkle_roots = BlockMerkleRoots::from_transactions(&b.txns);

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
    let dr_output = example_data_request_output(2, 200, 20);
    let (dr_pool, dr_pointer, rewarded, slashed, error_witnesses, _dr_pkh, _change, reward) =
        dr_pool_with_dr_in_tally_stage(dr_output, 2, 2, 0, reveal_value, vec![]);
    assert_eq!(reward, 200 + ONE_WIT);

    let tally_value = vec![0x00];
    let vt0 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[0],
        value: reward,
    };
    let vt1 = ValueTransferOutput {
        time_lock: 0,
        pkh: rewarded[1],
        value: reward,
    };
    let tally_transaction = TallyTransaction::new(
        dr_pointer,
        tally_value.clone(),
        vec![vt0.clone(), vt1.clone()],
        slashed.clone(),
        error_witnesses.clone(),
    );
    let tally_transaction2 = TallyTransaction::new(
        dr_pointer,
        tally_value,
        vec![vt1, vt0],
        slashed,
        error_witnesses,
    );

    assert_ne!(tally_transaction.hash(), tally_transaction2.hash());

    let x = test_block_with_drpool(
        |b| {
            // We include two tallies with same dr_pointer
            b.txns.tally_txns.push(tally_transaction.clone());
            b.txns.tally_txns.push(tally_transaction2.clone());

            b.txns.mint = MintTransaction::new(
                b.txns.mint.epoch,
                vec![ValueTransferOutput {
                    time_lock: 0,
                    value: transaction_outputs_sum(&b.txns.mint.outputs).unwrap() + 100, // tally_fee is 100
                    ..b.txns.mint.outputs[0]
                }],
            );

            b.block_header.merkle_roots = BlockMerkleRoots::from_transactions(&b.txns);

            true
        },
        dr_pool,
    );

    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DuplicatedTally { dr_pointer },
    );
}

#[test]
fn block_before_and_after_hard_fork() {
    let mut dr_pool = DataRequestPool::default();
    let dro = DataRequestOutput {
        witness_reward: 1000,
        witnesses: 100,
        commit_and_reveal_fee: 50,
        min_consensus_percentage: 51,
        data_request: example_data_request_before_wip19(),
        collateral: ONE_WIT,
    };
    let dr_body = DRTransactionBody::new(vec![], vec![], dro.clone());
    let drs = sign_tx(PRIV_KEY_1, &dr_body);
    let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
    let dr_epoch = 0;
    dr_pool
        .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
        .unwrap();

    // Another data request to insert in the block
    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: 110020,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);
    let vti = Input::new(utxo_set.iter().next().unwrap().0);
    let dr_tx_body = DRTransactionBody::new(vec![vti], vec![], dro);
    let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
    let dr_transaction = DRTransaction::new(dr_tx_body, vec![drs]);

    let x = test_block_with_drpool_and_utxo_set(
        |b| {
            b.txns.data_request_txns.push(dr_transaction.clone());

            b.txns.mint = MintTransaction::new(
                b.txns.mint.epoch,
                vec![ValueTransferOutput {
                    time_lock: 0,
                    value: transaction_outputs_sum(&b.txns.mint.outputs).unwrap() + 20,
                    ..b.txns.mint.outputs[0]
                }],
            );

            b.block_header.merkle_roots = BlockMerkleRoots::from_transactions(&b.txns);

            true
        },
        dr_pool.clone(),
        utxo_set.clone(),
        FIRST_HARD_FORK - 1,
    );
    x.unwrap();

    let x = test_block_with_drpool_and_utxo_set(
        |b| {
            b.txns.data_request_txns.push(dr_transaction.clone());

            b.txns.mint = MintTransaction::new(
                b.txns.mint.epoch,
                vec![ValueTransferOutput {
                    time_lock: 0,
                    value: transaction_outputs_sum(&b.txns.mint.outputs).unwrap() + 20,
                    ..b.txns.mint.outputs[0]
                }],
            );

            b.block_header.merkle_roots = BlockMerkleRoots::from_transactions(&b.txns);

            true
        },
        dr_pool,
        utxo_set,
        E - 1,
    );
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::TotalDataRequestWeightLimitExceeded {
            weight: 127701,
            max_weight: 80000
        },
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
        b.block_sig = sign_tx(PRIV_KEY_2, &b.block_header);
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
fn block_change_hash_prev_vrf() {
    let x = test_block(|b| {
        let fake_hash = Hash::default();
        // Re-create a valid VRF proof
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let secret_key = SecretKey {
            bytes: Protected::from(vec![0xcd; 32]),
        };
        let vrf_input = CheckpointVRF {
            checkpoint: 1000,
            hash_prev_vrf: fake_hash,
        };
        b.block_header.proof = BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap();
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

#[test]
fn block_change_signals() {
    // Check that an attacker cannot change the version field of a signed block, because that will
    // invalidate the signature
    let x = test_block(|b| {
        // Flip one bit of the signals field
        b.block_header.signals ^= 1;

        // Do not sign the block again
        // If the miner changes the signals field and then signs the block, the block will be valid
        false
    });

    // The block should be invalid now because the block hash has changed
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
        BlockError::VerifySignatureFail {
            hash: Default::default()
        },
    );
}

///////////////////////////////////////////////////////////////////////////////
// Block transaction tests: multiple blocks in sequence
///////////////////////////////////////////////////////////////////////////////
fn test_blocks(txns: Vec<(BlockTransactions, u64)>) -> Result<(), failure::Error> {
    test_blocks_with_limits(
        txns,
        MAX_VT_WEIGHT,
        MAX_DR_WEIGHT,
        GENESIS_BLOCK_HASH.parse().unwrap(),
    )
}

fn test_blocks_with_limits(
    txns: Vec<(BlockTransactions, u64)>,
    max_vt_weight: u32,
    max_dr_weight: u32,
    genesis_block_hash: Hash,
) -> Result<(), failure::Error> {
    if txns.len() > 1 {
        // FIXME(#685): add sequence validations
        unimplemented!();
    }

    let dr_pool = DataRequestPool::default();
    let vrf = &mut VrfCtx::secp256k1().unwrap();
    let rep_eng = ReputationEngine::new(100);
    let mut utxo_set = UnspentOutputsPool::default();
    let block_number = 0;

    let consensus_constants = ConsensusConstants {
        checkpoint_zero_timestamp: 0,
        collateral_minimum: 1,
        bootstrapping_committee: vec![],
        collateral_age: 1,
        superblock_period: 0,
        mining_backup_factor: 8,
        bootstrap_hash: BOOTSTRAP_HASH.parse().unwrap(),
        genesis_hash: genesis_block_hash,
        max_dr_weight,
        activity_period: 0,
        reputation_expire_alpha_diff: 0,
        reputation_issuance: 0,
        reputation_issuance_stop: 0,
        max_vt_weight,
        checkpoints_period: 0,
        reputation_penalization_factor: 0.0,
        mining_replication_factor: 0,
        extra_rounds: 0,
        minimum_difficulty: 0,
        epochs_with_minimum_difficulty: 0,
        superblock_signing_committee_size: 100,
        superblock_committee_decreasing_period: 100,
        superblock_committee_decreasing_step: 5,
        initial_block_reward: INITIAL_BLOCK_REWARD,
        halving_period: HALVING_PERIOD,
    };

    // Insert output to utxo
    let output1 = ValueTransferOutput {
        time_lock: 0,
        pkh: MY_PKH_1.parse().unwrap(),
        value: 1_000_000,
    };
    //let tx_output1 = VTTransactionBody::new(vec![], vec![output1.clone()]);
    //let output1_pointer = OutputPointer { transaction_id: tx_output1.hash(), output_index: 0 };
    let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
    utxo_set.insert(output1_pointer, output1.clone(), block_number);
    let output2_pointer = MILLION_TX_OUTPUT2.parse().unwrap();
    utxo_set.insert(output2_pointer, output1, block_number);

    let secret_key = SecretKey {
        bytes: Protected::from(PRIV_KEY_1.to_vec()),
    };
    let mut current_epoch = 1000;
    let mut last_block_hash = LAST_BLOCK_HASH.parse().unwrap();
    let last_vrf_input = LAST_VRF_INPUT.parse().unwrap();
    let my_pkh = PublicKeyHash::default();

    for (mut txns, fees) in txns {
        // Rebuild mint
        txns.mint = MintTransaction::new(
            current_epoch,
            vec![ValueTransferOutput {
                time_lock: 0,
                pkh: my_pkh,
                value: block_reward(current_epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD) + fees,
            }],
        );

        let vrf_input = CheckpointVRF {
            checkpoint: current_epoch,
            hash_prev_vrf: last_vrf_input,
        };

        let chain_beacon = CheckpointBeacon {
            checkpoint: current_epoch,
            hash_prev_block: last_block_hash,
        };
        let block_beacon = CheckpointBeacon {
            checkpoint: current_epoch,
            hash_prev_block: last_block_hash,
        };
        let block_header = BlockHeader {
            merkle_roots: BlockMerkleRoots::from_transactions(&txns),
            beacon: block_beacon,
            proof: BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap(),
            ..Default::default()
        };
        let block_sig = KeyedSignature::default();
        let mut b = Block::new(block_header, block_sig, txns);

        b.block_sig = sign_tx(PRIV_KEY_1, &b.block_header);

        let mut signatures_to_verify = vec![];

        // Validate block VRF
        validate_block(
            &b,
            current_epoch,
            vrf_input,
            chain_beacon,
            &mut signatures_to_verify,
            &rep_eng,
            &consensus_constants,
            &current_active_wips(),
        )?;
        verify_signatures_test(signatures_to_verify)?;
        let mut signatures_to_verify = vec![];

        // Do the expensive validation
        validate_block_transactions(
            &utxo_set,
            &dr_pool,
            &b,
            vrf_input,
            &mut signatures_to_verify,
            &rep_eng,
            EpochConstants::default(),
            block_number,
            &consensus_constants,
            &current_active_wips(),
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
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
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
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
        let vt_tx1 = VTTransaction::new(vt_body, vec![vts]);

        let vto0 = ValueTransferOutput {
            time_lock: 0,
            pkh: Default::default(),
            value: 1,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0]);
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
        let vt_tx2 = VTTransaction::new(vt_body, vec![vts]);

        (
            BlockTransactions {
                value_transfer_txns: vec![vt_tx1, vt_tx2],
                ..BlockTransactions::default()
            },
            (1_000_000 - 1) * 2,
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
fn block_add_1_vtt_2_same_input() {
    let t0 = {
        let vto0 = ValueTransferOutput {
            time_lock: 0,
            pkh: Default::default(),
            value: 1,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer); 2], vec![vto0]);
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
        let vt_tx1 = VTTransaction::new(vt_body, vec![vts; 2]);

        (
            BlockTransactions {
                value_transfer_txns: vec![vt_tx1],
                ..BlockTransactions::default()
            },
            1_000_000 * 2 - 1,
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
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
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
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
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
            pkh: MY_PKH_1.parse().unwrap(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let dr_tx_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0], dr_output);
        let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
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
            pkh: MY_PKH_1.parse().unwrap(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let dr_tx_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0], dr_output);
        let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
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
            pkh: MY_PKH_1.parse().unwrap(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let dr_tx_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0], dr_output);
        let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
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
            pkh: MY_PKH_1.parse().unwrap(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let dr_tx_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0], dr_output);
        let drs = sign_tx(PRIV_KEY_1, &dr_tx_body);
        let dr_tx = DRTransaction::new(dr_tx_body, vec![drs]);

        let vto0 = ValueTransferOutput {
            time_lock: 0,
            pkh: Default::default(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0]);
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
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
            pkh: MY_PKH_1.parse().unwrap(),
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
    b.block_sig = sign_tx(PRIV_KEY_1, &b);

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
    let vrf_input = CheckpointVRF::default();

    let rep_eng = ReputationEngine::new(100);

    let current_epoch = 0;
    // If this was bootstrap hash, the validation would pass:
    let last_block_hash = b.hash();
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: last_block_hash,
    };

    let consensus_constants = ConsensusConstants {
        checkpoint_zero_timestamp: 0,
        collateral_minimum: 1,
        bootstrapping_committee: vec![],
        collateral_age: 1,
        superblock_period: 0,
        mining_backup_factor: 1,
        bootstrap_hash,
        genesis_hash: b.hash(),
        max_dr_weight: MAX_DR_WEIGHT,
        activity_period: 0,
        reputation_expire_alpha_diff: 0,
        reputation_issuance: 0,
        reputation_issuance_stop: 0,
        max_vt_weight: MAX_VT_WEIGHT,
        checkpoints_period: 0,
        reputation_penalization_factor: 0.0,
        mining_replication_factor: 0,
        extra_rounds: 0,
        minimum_difficulty: 0,
        epochs_with_minimum_difficulty: 0,
        superblock_signing_committee_size: 100,
        superblock_committee_decreasing_period: 100,
        superblock_committee_decreasing_step: 5,
        initial_block_reward: INITIAL_BLOCK_REWARD,
        halving_period: HALVING_PERIOD,
    };
    let mut signatures_to_verify = vec![];

    // Validate block
    let x = validate_block(
        &b,
        current_epoch,
        vrf_input,
        chain_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        &consensus_constants,
        &current_active_wips(),
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
    let outputs = vec![ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: u64::max_value(),
        time_lock: 0,
    }];

    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
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
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: bootstrap_hash,
    };

    let consensus_constants = ConsensusConstants {
        checkpoint_zero_timestamp: 0,
        bootstrapping_committee: vec![],
        collateral_minimum: 1,
        collateral_age: 1,
        superblock_period: 0,
        mining_backup_factor: 1,
        bootstrap_hash,
        genesis_hash: b.hash(),
        max_dr_weight: MAX_DR_WEIGHT,
        activity_period: 0,
        reputation_expire_alpha_diff: 0,
        reputation_issuance: 0,
        reputation_issuance_stop: 0,
        max_vt_weight: MAX_VT_WEIGHT,
        checkpoints_period: 0,
        reputation_penalization_factor: 0.0,
        mining_replication_factor: 0,
        extra_rounds: 0,
        minimum_difficulty: 0,
        epochs_with_minimum_difficulty: 0,
        superblock_signing_committee_size: 100,
        superblock_committee_decreasing_period: 100,
        superblock_committee_decreasing_step: 5,
        initial_block_reward: INITIAL_BLOCK_REWARD,
        halving_period: HALVING_PERIOD,
    };
    let vrf_input = CheckpointVRF::default();
    let mut signatures_to_verify = vec![];

    // Validate block
    validate_block(
        &b,
        current_epoch,
        vrf_input,
        chain_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        &consensus_constants,
        &current_active_wips(),
    )
    .unwrap();
    assert_eq!(signatures_to_verify, vec![]);
    let mut signatures_to_verify = vec![];

    // Do the expensive validation
    let x = validate_block_transactions(
        &utxo_set,
        &dr_pool,
        &b,
        vrf_input,
        &mut signatures_to_verify,
        &rep_eng,
        EpochConstants::default(),
        block_number,
        &consensus_constants,
        &current_active_wips(),
    );
    assert_eq!(signatures_to_verify, vec![]);
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::GenesisValueOverflow {
            max_total_value: u64::max_value()
                - total_block_reward(INITIAL_BLOCK_REWARD, HALVING_PERIOD),
        },
    );
}

#[test]
fn genesis_block_full_validate() {
    let bootstrap_hash = BOOTSTRAP_HASH.parse().unwrap();
    let b = Block::genesis(bootstrap_hash, vec![]);
    let vrf_input = CheckpointVRF::default();

    let dr_pool = DataRequestPool::default();
    let rep_eng = ReputationEngine::new(100);
    let utxo_set = UnspentOutputsPool::default();

    let current_epoch = 0;
    let block_number = 0;
    let chain_beacon = CheckpointBeacon {
        checkpoint: current_epoch,
        hash_prev_block: bootstrap_hash,
    };
    let mut signatures_to_verify = vec![];

    let consensus_constants = ConsensusConstants {
        checkpoint_zero_timestamp: 0,
        bootstrapping_committee: vec![],
        collateral_minimum: 1,
        collateral_age: 1,
        superblock_period: 0,
        mining_backup_factor: 1,
        bootstrap_hash,
        genesis_hash: b.hash(),
        max_dr_weight: MAX_DR_WEIGHT,
        activity_period: 0,
        reputation_expire_alpha_diff: 0,
        reputation_issuance: 0,
        reputation_issuance_stop: 0,
        max_vt_weight: MAX_VT_WEIGHT,
        checkpoints_period: 0,
        reputation_penalization_factor: 0.0,
        mining_replication_factor: 0,
        extra_rounds: 0,
        minimum_difficulty: 0,
        epochs_with_minimum_difficulty: 0,
        superblock_signing_committee_size: 100,
        superblock_committee_decreasing_period: 100,
        superblock_committee_decreasing_step: 5,
        initial_block_reward: INITIAL_BLOCK_REWARD,
        halving_period: HALVING_PERIOD,
    };

    // Validate block
    validate_block(
        &b,
        current_epoch,
        vrf_input,
        chain_beacon,
        &mut signatures_to_verify,
        &rep_eng,
        &consensus_constants,
        &current_active_wips(),
    )
    .unwrap();
    assert_eq!(signatures_to_verify, vec![]);
    let mut signatures_to_verify = vec![];

    // Do the expensive validation
    validate_block_transactions(
        &utxo_set,
        &dr_pool,
        &b,
        vrf_input,
        &mut signatures_to_verify,
        &rep_eng,
        EpochConstants::default(),
        block_number,
        &consensus_constants,
        &current_active_wips(),
    )
    .unwrap();
    assert_eq!(signatures_to_verify, vec![]);
}

#[test]
fn validate_block_transactions_uses_block_number_in_utxo_diff() {
    // Check that the UTXO diff returned by validate_block_transactions respects the block number
    let block_number = 1234;

    let utxo_diff = {
        let consensus_constants = ConsensusConstants {
            checkpoint_zero_timestamp: 0,
            bootstrapping_committee: vec![],
            checkpoints_period: 0,
            collateral_minimum: 1,
            collateral_age: 1,
            superblock_period: 0,
            genesis_hash: GENESIS_BLOCK_HASH.parse().unwrap(),
            max_dr_weight: MAX_DR_WEIGHT,
            activity_period: 0,
            reputation_expire_alpha_diff: 0,
            reputation_issuance: 0,
            reputation_issuance_stop: 0,
            reputation_penalization_factor: 0.0,
            mining_backup_factor: 0,
            max_vt_weight: MAX_VT_WEIGHT,
            bootstrap_hash: Default::default(),
            mining_replication_factor: 0,
            extra_rounds: 0,
            minimum_difficulty: 0,
            epochs_with_minimum_difficulty: 0,
            superblock_signing_committee_size: 100,
            superblock_committee_decreasing_period: 100,
            superblock_committee_decreasing_step: 5,
            initial_block_reward: INITIAL_BLOCK_REWARD,
            halving_period: HALVING_PERIOD,
        };
        let dr_pool = DataRequestPool::default();
        let vrf = &mut VrfCtx::secp256k1().unwrap();
        let rep_eng = ReputationEngine::new(100);
        let utxo_set = UnspentOutputsPool::default();

        let secret_key = SecretKey {
            bytes: Protected::from(PRIV_KEY_1.to_vec()),
        };
        let current_epoch = 1000;
        let vrf_input = CheckpointVRF::default();
        let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();
        let block_beacon = CheckpointBeacon {
            checkpoint: current_epoch,
            hash_prev_block: last_block_hash,
        };
        let my_pkh = PublicKeyHash::default();

        let txns = BlockTransactions {
            mint: MintTransaction::new(
                current_epoch,
                vec![ValueTransferOutput {
                    time_lock: 0,
                    pkh: my_pkh,
                    value: block_reward(current_epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD),
                }],
            ),
            ..BlockTransactions::default()
        };

        let block_header = BlockHeader {
            merkle_roots: BlockMerkleRoots::from_transactions(&txns),
            beacon: block_beacon,
            proof: BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap(),
            ..Default::default()
        };
        let block_sig = sign_tx(PRIV_KEY_1, &block_header);
        let b = Block::new(block_header, block_sig, txns);
        let mut signatures_to_verify = vec![];

        validate_block_transactions(
            &utxo_set,
            &dr_pool,
            &b,
            vrf_input,
            &mut signatures_to_verify,
            &rep_eng,
            EpochConstants::default(),
            block_number,
            &consensus_constants,
            &current_active_wips(),
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
            utxo_set.included_in_block_number(&output_pointer),
            Some(block_number)
        );
    }
}

#[test]
fn validate_commit_transactions_included_in_utxo_diff() {
    // Check that the collateral from commit transactions is removed from the UTXO set,
    // and the change outputs from commit transactions are included into the UTXO set
    let block_number = 100_000;
    let collateral_value = ONE_WIT;
    let change_value = 250;

    let change_vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: change_value,
        time_lock: 0,
    };
    let commit_tx_hash;
    let mint_tx_hash;
    let mint_vto;

    let vto = ValueTransferOutput {
        pkh: MY_PKH_1.parse().unwrap(),
        value: collateral_value + change_value,
        time_lock: 0,
    };
    let utxo_set = build_utxo_set_with_mint(vec![vto], None, vec![]);

    let utxo_diff = {
        let output = utxo_set.iter().next().unwrap().0;
        let vti = Input::new(output);

        let mut dr_pool = DataRequestPool::default();
        let vrf = &mut VrfCtx::secp256k1().unwrap();
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
        let drs = sign_tx(PRIV_KEY_1, &dr_body);
        let dr_transaction = DRTransaction::new(dr_body, vec![drs]);
        let dr_hash = dr_transaction.hash();
        assert_eq!(dr_hash, DR_HASH.parse().unwrap());
        let dr_epoch = 0;
        dr_pool
            .process_data_request(&dr_transaction, dr_epoch, &Hash::default())
            .unwrap();

        let secret_key = SecretKey {
            bytes: Protected::from(vec![0xcd; 32]),
        };
        let current_epoch = 1000;
        let last_block_hash = LAST_BLOCK_HASH.parse().unwrap();
        let last_vrf_input = LAST_VRF_INPUT.parse().unwrap();

        let block_beacon = CheckpointBeacon {
            checkpoint: current_epoch,
            hash_prev_block: last_block_hash,
        };

        let vrf_input = CheckpointVRF {
            checkpoint: current_epoch,
            hash_prev_vrf: last_vrf_input,
        };

        let my_pkh = PublicKeyHash::default();

        let mut txns = BlockTransactions::default();
        mint_vto = ValueTransferOutput {
            time_lock: 0,
            pkh: my_pkh,
            value: block_reward(current_epoch, INITIAL_BLOCK_REWARD, HALVING_PERIOD),
        };
        txns.mint = MintTransaction::new(current_epoch, vec![mint_vto.clone()]);
        mint_tx_hash = txns.mint.hash();

        // Insert valid proof
        let mut cb = CommitTransactionBody::default();
        cb.dr_pointer = dr_hash;
        cb.proof =
            DataRequestEligibilityClaim::create(vrf, &secret_key, vrf_input, dr_hash).unwrap();

        let consensus_constants = ConsensusConstants {
            checkpoint_zero_timestamp: 0,
            bootstrapping_committee: vec![],
            checkpoints_period: 0,
            collateral_minimum: 1,
            collateral_age: 1,
            superblock_period: 0,
            genesis_hash: GENESIS_BLOCK_HASH.parse().unwrap(),
            max_dr_weight: MAX_DR_WEIGHT,
            activity_period: 0,
            reputation_expire_alpha_diff: 0,
            reputation_issuance: 0,
            reputation_issuance_stop: 0,
            reputation_penalization_factor: 0.0,
            mining_backup_factor: 0,
            max_vt_weight: MAX_VT_WEIGHT,
            bootstrap_hash: Default::default(),
            mining_replication_factor: 0,
            extra_rounds: 0,
            minimum_difficulty: 1,
            epochs_with_minimum_difficulty: 0,
            superblock_signing_committee_size: 100,
            superblock_committee_decreasing_period: 100,
            superblock_committee_decreasing_step: 5,
            initial_block_reward: INITIAL_BLOCK_REWARD,
            halving_period: HALVING_PERIOD,
        };

        let (inputs, outputs) = (vec![vti], vec![change_vto.clone()]);
        cb.collateral = inputs;
        cb.outputs = outputs;

        // Sign commitment
        let cs = sign_tx(PRIV_KEY_1, &cb);
        let c_tx = CommitTransaction::new(cb, vec![cs]);
        commit_tx_hash = c_tx.hash();

        txns.commit_txns.push(c_tx);

        let block_header = BlockHeader {
            merkle_roots: BlockMerkleRoots::from_transactions(&txns),
            beacon: block_beacon,
            proof: BlockEligibilityClaim::create(vrf, &secret_key, vrf_input).unwrap(),
            ..Default::default()
        };
        let block_sig = sign_tx(PRIV_KEY_1, &block_header);
        let b = Block::new(block_header, block_sig, txns);
        let mut signatures_to_verify = vec![];

        validate_block_transactions(
            &utxo_set,
            &dr_pool,
            &b,
            vrf_input,
            &mut signatures_to_verify,
            &rep_eng,
            EpochConstants::default(),
            block_number,
            &consensus_constants,
            &current_active_wips(),
        )
        .unwrap()
    };

    // The original UTXO set contained one mint transaction
    assert_eq!(utxo_set.iter().count(), 1);
    // Apply the UTXO diff to the original UTXO set
    let mut utxo_set = utxo_set;
    utxo_diff.apply(&mut utxo_set);

    // The expected state of the UTXO set is:
    // * A new mint transaction for the block miner
    // * The change output of the collateral used in the commit transaction
    let mut expected_utxo_set = build_utxo_set_with_mint(vec![], None, vec![]);
    let mint_output_pointer = OutputPointer {
        transaction_id: mint_tx_hash,
        output_index: 0,
    };
    expected_utxo_set.insert(mint_output_pointer, mint_vto, block_number);
    let change_output_pointer = OutputPointer {
        transaction_id: commit_tx_hash,
        output_index: 0,
    };
    expected_utxo_set.insert(change_output_pointer, change_vto, 0);

    // In total, 2 outputs
    assert_eq!(expected_utxo_set.iter().count(), 2);

    let utxos: Vec<_> = utxo_set.iter().sorted_by(|a, b| a.0.cmp(&b.0)).collect();
    let expected_utxos: Vec<_> = expected_utxo_set
        .iter()
        .sorted_by(|a, b| a.0.cmp(&b.0))
        .collect();
    assert_eq!(utxos, expected_utxos);
}

#[test]
fn validate_required_tally_not_found() {
    let dr_pointer = Hash::default();
    let dr_state = DataRequestState {
        stage: DataRequestStage::TALLY,
        ..DataRequestState::default()
    };

    let mut dr_pool = DataRequestPool::default();
    dr_pool.data_request_pool.insert(dr_pointer, dr_state);

    let b = Block::default();

    let e = validate_block_transactions(
        &UnspentOutputsPool::default(),
        &dr_pool,
        &b,
        CheckpointVRF::default(),
        &mut vec![],
        &ReputationEngine::new(1000),
        EpochConstants::default(),
        100,
        &ConsensusConstants::default(),
        &current_active_wips(),
    )
    .unwrap_err();

    assert_eq!(
        e.downcast::<BlockError>().unwrap(),
        BlockError::MissingExpectedTallies {
            count: 1,
            block_hash: b.hash()
        },
    );
}

#[test]
fn validate_vt_weight_overflow() {
    let t0 = {
        let vto0 = ValueTransferOutput {
            time_lock: 0,
            pkh: Default::default(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0.clone()]);
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);
        assert_eq!(vt_tx.weight(), 493);

        let output2_pointer = MILLION_TX_OUTPUT2.parse().unwrap();
        let vt_body2 = VTTransactionBody::new(vec![Input::new(output2_pointer)], vec![vto0]);
        let vts2 = sign_tx(PRIV_KEY_1, &vt_body2);
        let vt_tx2 = VTTransaction::new(vt_body2, vec![vts2]);
        assert_eq!(vt_tx2.weight(), 493);

        (
            BlockTransactions {
                value_transfer_txns: vec![vt_tx, vt_tx2],
                ..BlockTransactions::default()
            },
            2_000_000 - 2 * 10,
        )
    };
    let x = test_blocks_with_limits(
        vec![t0],
        2 * 493 - 1,
        0,
        GENESIS_BLOCK_HASH.parse().unwrap(),
    );
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::TotalValueTransferWeightLimitExceeded {
            weight: 2 * 493,
            max_weight: 2 * 493 - 1,
        },
    );
}

#[test]
fn validate_vt_weight_valid() {
    let t0 = {
        let vto0 = ValueTransferOutput {
            time_lock: 0,
            pkh: Default::default(),
            value: 10,
        };
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let vt_body = VTTransactionBody::new(vec![Input::new(output1_pointer)], vec![vto0.clone()]);
        let vts = sign_tx(PRIV_KEY_1, &vt_body);
        let vt_tx = VTTransaction::new(vt_body, vec![vts]);
        assert_eq!(vt_tx.weight(), 493);

        let output2_pointer = MILLION_TX_OUTPUT2.parse().unwrap();
        let vt_body2 = VTTransactionBody::new(vec![Input::new(output2_pointer)], vec![vto0]);
        let vts2 = sign_tx(PRIV_KEY_1, &vt_body2);
        let vt_tx2 = VTTransaction::new(vt_body2, vec![vts2]);
        assert_eq!(vt_tx2.weight(), 493);

        (
            BlockTransactions {
                value_transfer_txns: vec![vt_tx, vt_tx2],
                ..BlockTransactions::default()
            },
            2_000_000 - 2 * 10,
        )
    };
    let x = test_blocks_with_limits(vec![t0], 2 * 493, 0, GENESIS_BLOCK_HASH.parse().unwrap());
    x.unwrap();
}

#[test]
fn validate_vt_weight_genesis_valid() {
    let new_genesis = "116e271cbda2c625ccc189a4b93b6d0e96063dd9b75258dc47acaac86cd19ceb";
    let t0 = {
        let vto0 = ValueTransferOutput {
            time_lock: 0,
            pkh: Default::default(),
            value: 10,
        };

        let vt_body = VTTransactionBody::new(vec![], vec![vto0]);
        let vt_tx = VTTransaction::new(vt_body, vec![]);

        assert_eq!(vt_tx.weight(), 360);

        (
            BlockTransactions {
                value_transfer_txns: vec![vt_tx],
                ..BlockTransactions::default()
            },
            1_000_000 - 10,
        )
    };
    let x = test_blocks_with_limits(vec![t0], 0, 0, new_genesis.parse().unwrap());
    x.unwrap();
}

#[test]
fn validate_dr_weight_overflow() {
    let t0 = {
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let output2_pointer = MILLION_TX_OUTPUT2.parse().unwrap();
        let dro = example_data_request_output(2, 1, 0);
        let dr_value = dro.checked_total_value().unwrap();

        let dr_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![], dro.clone());
        let drs = sign_tx(PRIV_KEY_1, &dr_body);
        let dr_tx = DRTransaction::new(dr_body, vec![drs]);
        assert_eq!(dr_tx.weight(), 1589);

        let dr_body2 = DRTransactionBody::new(vec![Input::new(output2_pointer)], vec![], dro);
        let drs2 = sign_tx(PRIV_KEY_1, &dr_body2);
        let dr_tx2 = DRTransaction::new(dr_body2, vec![drs2]);
        assert_eq!(dr_tx2.weight(), 1589);

        (
            BlockTransactions {
                data_request_txns: vec![dr_tx, dr_tx2],
                ..BlockTransactions::default()
            },
            2_000_000 - 2 * dr_value,
        )
    };
    let x = test_blocks_with_limits(
        vec![t0],
        0,
        2 * 1589 - 1,
        GENESIS_BLOCK_HASH.parse().unwrap(),
    );
    assert_eq!(
        x.unwrap_err().downcast::<BlockError>().unwrap(),
        BlockError::TotalDataRequestWeightLimitExceeded {
            weight: 2 * 1589,
            max_weight: 2 * 1589 - 1,
        },
    );
}

// This test evaluates the theoretical limit of witnesses for a MAX_DR_WEIGHT of 80_000
#[test]
fn validate_dr_weight_overflow_126_witnesses() {
    let dro = example_data_request_output(126, 1, 0);
    let t0 = {
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let dr_value = dro.checked_total_value().unwrap();

        let dr_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![], dro.clone());
        let drs = sign_tx(PRIV_KEY_1, &dr_body);
        let dr_tx = DRTransaction::new(dr_body, vec![drs]);

        assert_eq!(dr_tx.weight(), 80453);

        (
            BlockTransactions {
                data_request_txns: vec![dr_tx],
                ..BlockTransactions::default()
            },
            1_000_000 - dr_value,
        )
    };
    let x = test_blocks_with_limits(
        vec![t0],
        0,
        MAX_DR_WEIGHT,
        GENESIS_BLOCK_HASH.parse().unwrap(),
    );
    assert_eq!(
        x.unwrap_err().downcast::<TransactionError>().unwrap(),
        TransactionError::DataRequestWeightLimitExceeded {
            weight: 80453,
            max_weight: MAX_DR_WEIGHT,
            dr_output: dro,
        },
    );
}

#[test]
fn validate_dr_weight_valid() {
    let t0 = {
        let output1_pointer = MILLION_TX_OUTPUT.parse().unwrap();
        let output2_pointer = MILLION_TX_OUTPUT2.parse().unwrap();
        let dro = example_data_request_output(2, 1, 0);
        let dr_value = dro.checked_total_value().unwrap();

        let dr_body =
            DRTransactionBody::new(vec![Input::new(output1_pointer)], vec![], dro.clone());
        let drs = sign_tx(PRIV_KEY_1, &dr_body);
        let dr_tx = DRTransaction::new(dr_body, vec![drs]);
        assert_eq!(dr_tx.weight(), 1589);

        let dr_body2 = DRTransactionBody::new(vec![Input::new(output2_pointer)], vec![], dro);
        let drs2 = sign_tx(PRIV_KEY_1, &dr_body2);
        let dr_tx2 = DRTransaction::new(dr_body2, vec![drs2]);
        assert_eq!(dr_tx2.weight(), 1589);

        (
            BlockTransactions {
                data_request_txns: vec![dr_tx, dr_tx2],
                ..BlockTransactions::default()
            },
            2_000_000 - 2 * dr_value,
        )
    };
    let x = test_blocks_with_limits(vec![t0], 0, 2 * 1605, GENESIS_BLOCK_HASH.parse().unwrap());
    x.unwrap();
}
