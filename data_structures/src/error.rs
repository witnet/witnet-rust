//! Error type definitions for the data structure module.

use failure::Fail;
use std::num::ParseIntError;

use crate::chain::{
    DataRequestOutput, Epoch, Hash, HashParseError, OutputPointer, PublicKeyHash, RADType,
};

/// The error type for operations on a [`ChainInfo`](ChainInfo)
#[derive(Debug, PartialEq, Fail)]
pub enum ChainInfoError {
    /// Errors when try to use a None value for ChainInfo
    #[fail(display = "No ChainInfo loaded in ChainManager")]
    ChainInfoNotFound,
}

/// Error in builders functions
#[derive(Debug, PartialEq, Fail)]
pub enum BuildersError {
    /// No inventory vectors available to create a Inventory Announcement message
    #[fail(display = "No inventory vectors available to create a Inventory Announcement message")]
    NoInvVectorsAnnouncement,
    /// No inventory vectors available to create a Inventory Request message
    #[fail(display = "No inventory vectors available to create a Inventory Request message")]
    NoInvVectorsRequest,
}

/// The error type for operations on a [`Transaction`](Transaction)
#[derive(Debug, PartialEq, Fail)]
pub enum TransactionError {
    #[fail(display = "The transaction is invalid")]
    NotValidTransaction,
    #[fail(display = "Sum of fees overflows")]
    FeeOverflow,
    #[fail(display = "Sum of input values overflows")]
    InputValueOverflow,
    #[fail(display = "Sum of output values overflows")]
    OutputValueOverflow,
    /// The transaction creates value
    #[fail(display = "Transaction creates value (its fee is negative)")]
    NegativeFee,
    /// An output with the given index wasn't found in a transaction.
    #[fail(display = "Output not found: {}", output)]
    OutputNotFound { output: OutputPointer },
    #[fail(display = "Data Request not found: {}", hash)]
    DataRequestNotFound { hash: Hash },
    #[fail(display = "Commit transaction has a invalid Proof of Eligibility")]
    InvalidDataRequestPoe,
    #[fail(
        display = "The data request eligibility claim VRF proof hash is greater than the target hash: {} > {}",
        vrf_hash, target_hash
    )]
    DataRequestEligibilityDoesNotMeetTarget { vrf_hash: Hash, target_hash: Hash },
    #[fail(
        display = "Invalid tally change found: {}. Expected value: {}",
        change, expected_change
    )]
    InvalidTallyChange { change: u64, expected_change: u64 },
    #[fail(
        display = "Invalid witness reward found: {}. Expected value: {}",
        value, expected_value
    )]
    InvalidReward { value: u64, expected_value: u64 },
    #[fail(
        display = "In tally validation, the total amount is incorrect. Found: {}. Expected value: {}",
        value, expected_value
    )]
    InvalidTallyValue { value: u64, expected_value: u64 },
    #[fail(display = "Data Request witness reward must be greater than zero")]
    NoReward,
    #[fail(display = "Data Request witnesses number must be greater than zero")]
    InsufficientWitnesses,
    #[fail(
        display = "Mismatch between expected tally ({:?}) and miner tally ({:?})",
        expected_tally, miner_tally
    )]
    MismatchedConsensus {
        expected_tally: Vec<u8>,
        miner_tally: Vec<u8>,
    },
    #[fail(
        display = "Mismatching number of signatures ({}) and inputs ({})",
        signatures_n, inputs_n
    )]
    MismatchingSignaturesNumber { signatures_n: u8, inputs_n: u8 },
    /// Transaction verification process failed.
    #[fail(
        display = "Failed to verify the signature of transaction {}: {}",
        hash, msg
    )]
    VerifyTransactionSignatureFail { hash: Hash, msg: String },
    /// Signature not found
    #[fail(display = "Transaction signature not found")]
    SignatureNotFound,
    /// Public Key Hash does not match
    #[fail(
        display = "Public key hash mismatch: expected {} got {}",
        expected_pkh, signature_pkh
    )]
    PublicKeyHashMismatch {
        expected_pkh: PublicKeyHash,
        signature_pkh: PublicKeyHash,
    },
    /// Commit related to a reveal not found
    #[fail(display = "Commitment related to a reveal not found")]
    CommitNotFound,
    /// Reveal related to a tally not found
    #[fail(display = "Reveal related to a tally not found")]
    RevealNotFound,
    /// Commitment field in CommitTransaction does not match with RevealTransaction signature
    #[fail(
        display = "Commitment field in CommitTransaction does not match with RevealTransaction signature"
    )]
    MismatchedCommitment,
    /// No inputs when the transaction must have at least one
    #[fail(display = "Transaction {} cannot have zero inputs", tx_hash)]
    NoInputs { tx_hash: Hash },
    #[fail(
        display = "Genesis transaction should have 0 inputs, but has {} inputs",
        inputs_n
    )]
    InputsInGenesis { inputs_n: usize },
    #[fail(display = "Genesis transactions cannot have 0 outputs")]
    NoOutputsInGenesis,
    /// An output with zero value does not make sense
    #[fail(
        display = "Transaction {} has a zero value output at index {}",
        tx_hash, output_id
    )]
    ZeroValueOutput { tx_hash: Hash, output_id: usize },
    /// A dishonest witness has been rewarded
    #[fail(display = "A dishonest witness has been rewarded")]
    DishonestReward,
    /// This pkh was rewarded previously
    #[fail(display = "This pkh {} was rewarded previously", pkh)]
    MultipleRewards { pkh: PublicKeyHash },
    /// There are a different number of outputs than expected
    #[fail(
        display = "There are a different number of outputs ({}) than expected ({})",
        outputs, expected_outputs
    )]
    WrongNumberOutputs {
        outputs: usize,
        expected_outputs: usize,
    },
    /// Transaction has a time lock and cannot be included in this epoch
    #[fail(
        display = "Transaction cannot be included before {} but the block timestamp is {}",
        expected, current
    )]
    TimeLock { current: i64, expected: i64 },
    /// Value Transfer Output has an invalid time lock
    #[fail(
        display = "Value Transfer Output time_lock should be {}, but it is {}",
        expected, current
    )]
    InvalidTimeLock { current: u64, expected: u64 },
    /// This commit was already included
    #[fail(
        display = "Commit with pkh {} was already included for the data request {}",
        pkh, dr_pointer
    )]
    DuplicatedCommit {
        pkh: PublicKeyHash,
        dr_pointer: Hash,
    },
    /// This reveal was already included
    #[fail(
        display = "Reveal with pkh {} was already included for the data request {}",
        pkh, dr_pointer
    )]
    DuplicatedReveal {
        pkh: PublicKeyHash,
        dr_pointer: Hash,
    },
    /// This tally was already included
    #[fail(
        display = "Tally was already included for the data request {}",
        dr_pointer
    )]
    DuplicatedTally { dr_pointer: Hash },
    /// RadonReport not in Tally Stage
    #[fail(display = "RadonReport not in Tally Stage")]
    NoTallyStage,
    /// Minimum consensus percentage is invalid
    #[fail(
        display = "Minimum consensus percentage {} is invalid. Must be >50 and <100",
        value
    )]
    InvalidMinConsensus { value: u32 },
    /// Error when there is not enough balance to create a transaction
    #[fail(
        display = "Cannot build a transaction. Transaction value is greater than available balance: \
                   (Total Balance:{}, Available Balance:{}, Transaction value:{})",
        total_balance, available_balance, transaction_value
    )]
    NoMoney {
        total_balance: u64,
        available_balance: u64,
        transaction_value: u64,
    },
    /// Zero amount specified
    #[fail(display = "A transaction with zero value is invalid")]
    ZeroAmount,
    /// Incorrect count of out-of-consensus witnesses in Tally
    #[fail(
        display = "Incorrect count of out-of-consensus witnesses in Tally. Expected: {:?}, found: {:?}",
        expected, found
    )]
    MismatchingOutOfConsensusCount {
        expected: Vec<PublicKeyHash>,
        found: Vec<PublicKeyHash>,
    },
    /// Incorrect count of witnesses with errors in Tally
    #[fail(
        display = "Incorrect count of witnesses with errors in Tally. Expected: {:?}, found: {:?}",
        expected, found
    )]
    MismatchingErrorCount {
        expected: Vec<PublicKeyHash>,
        found: Vec<PublicKeyHash>,
    },
    /// Invalid collateral in data request
    #[fail(
        display = "The specified collateral ({} nwits), is less than the minimum required ({} nwits)",
        value, min
    )]
    InvalidCollateral { value: u64, min: u64 },
    /// Negative collateral in commit transaction
    #[fail(
        display = "Negative collateral in commit transaction. Input value: {}, output value: {}",
        input_value, output_value
    )]
    NegativeCollateral { input_value: u64, output_value: u64 },
    /// Incorrect collateral in commit transaction
    #[fail(
        display = "Incorrect collateral. Expected: {}, found: {}",
        expected, found
    )]
    IncorrectCollateral { expected: u64, found: u64 },
    /// Collateral in commit transaction is not mature enough
    #[fail(
        display = "Output {} used as input for collateralized commitment is not mature enough. Inputs of commitment transactions must be older than {} blocks, but this one was only {} blocks old",
        output, must_be_older_than, found
    )]
    CollateralNotMature {
        must_be_older_than: u32,
        found: u32,
        output: OutputPointer,
    },
    /// Collateral in commit transaction uses a different PKH than the commit VRF Proof
    #[fail(
        display = "Output {} used as input for collateralized commitment has pkh {} when the commit proof has pkh {}",
        output, output_pkh, proof_pkh
    )]
    CollateralPkhMismatch {
        output: OutputPointer,
        output_pkh: PublicKeyHash,
        proof_pkh: PublicKeyHash,
    },
    /// More than one output for the collateral change
    #[fail(display = "More than one output for the collateral change")]
    SeveralCommitOutputs,
    /// Value Transfer weight limit exceeded
    #[fail(
        display = "Value Transfer Transaction weight ({}) exceeds the limit ({})",
        weight, max_weight
    )]
    ValueTransferWeightLimitExceeded { weight: u32, max_weight: u32 },
    /// Data Request weight limit exceeded
    #[fail(
        display = "Data Request Transaction weight ({}) exceeds the limit ({})\n > {:?}",
        weight, max_weight, dr_output
    )]
    DataRequestWeightLimitExceeded {
        weight: u32,
        max_weight: u32,
        dr_output: DataRequestOutput,
    },
}

/// The error type for operations on a [`Block`](Block)
#[derive(Debug, PartialEq, Fail)]
pub enum BlockError {
    /// The block has no transactions in it.
    #[fail(display = "The block has no transactions")]
    Empty,
    /// The total value created by the mint transaction of the block,
    /// and the output value of the rest of the transactions, plus the
    /// block reward, don't add up
    #[fail(
        display = "The value of the mint transaction does not match the fees + reward of the block ({} != {} + {})",
        mint_value, fees_value, reward_value
    )]
    MismatchedMintValue {
        mint_value: u64,
        fees_value: u64,
        reward_value: u64,
    },
    #[fail(display = "MintTransaction was split in more than two 'ValueTransferOutput'")]
    TooSplitMint,
    #[fail(
        display = "Mint transaction has invalid epoch: mint {}, block {}",
        mint_epoch, block_epoch
    )]
    InvalidMintEpoch {
        mint_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[fail(display = "The block has an invalid PoE")]
    NotValidPoe,
    #[fail(
        display = "The block eligibility claim VRF proof hash is greater than the target hash: {} > {}",
        vrf_hash, target_hash
    )]
    BlockEligibilityDoesNotMeetTarget { vrf_hash: Hash, target_hash: Hash },
    #[fail(display = "The block has an invalid Merkle Tree")]
    NotValidMerkleTree,
    #[fail(
        display = "Block epoch from the future. Current epoch is: {}, block epoch is: {}",
        current_epoch, block_epoch
    )]
    BlockFromFuture {
        current_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[fail(
        display = "Ignoring block because its epoch ({}) is older than highest block checkpoint ({})",
        block_epoch, chain_epoch
    )]
    BlockOlderThanTip {
        chain_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[fail(
        display = "Ignoring block because previous hash (\"{}\") is different from our top block hash (\"{}\")",
        block_hash, our_hash
    )]
    PreviousHashMismatch { block_hash: Hash, our_hash: Hash },
    #[fail(
        display = "Ignoring genesis block because it is different from our expected genesis block:\nBlock:    `{}`\nExpected: `{}`",
        block, expected
    )]
    GenesisBlockMismatch { block: String, expected: String },
    #[fail(
        display = "Ignoring genesis block because its hash (\"{}\") is different from our expected genesis block hash (\"{}\")",
        block_hash, expected_hash
    )]
    GenesisBlockHashMismatch {
        block_hash: Hash,
        expected_hash: Hash,
    },
    #[fail(
        display = "Genesis block creates more value than allowed. Value cannot be greater than {}",
        max_total_value
    )]
    GenesisValueOverflow { max_total_value: u64 },
    #[fail(
        display = "Block candidate's epoch differs from current epoch ({} != {})",
        block_epoch, current_epoch
    )]
    CandidateFromDifferentEpoch {
        current_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[fail(
        display = "Commits in block ({}) are not equal to commits required ({})",
        commits, rf
    )]
    MismatchingCommitsNumber { commits: u32, rf: u32 },
    /// Block verification signature process failed.
    #[fail(display = "Failed to verify the signature of block {}", hash)]
    VerifySignatureFail { hash: Hash },
    /// Public Key Hash does not match
    #[fail(
        display = "Public key hash mismatch: VRF Proof PKH: {}, signature PKH: {}",
        proof_pkh, signature_pkh
    )]
    PublicKeyHashMismatch {
        proof_pkh: PublicKeyHash,
        signature_pkh: PublicKeyHash,
    },
    /// Value Transfer weight limit exceeded
    #[fail(
        display = "Total weight of Value Transfer Transactions in a block ({}) exceeds the limit ({})",
        weight, max_weight
    )]
    TotalValueTransferWeightLimitExceeded { weight: u32, max_weight: u32 },
    /// Data Request weight limit exceeded
    #[fail(
        display = "Total weight of Data Request Transactions in a block ({}) exceeds the limit ({})",
        weight, max_weight
    )]
    TotalDataRequestWeightLimitExceeded { weight: u32, max_weight: u32 },
    /// Missing expected tallies
    #[fail(
        display = "{} expected tally transactions are missing in block candidate {}",
        count, block_hash
    )]
    MissingExpectedTallies { count: usize, block_hash: Hash },
}

#[derive(Debug, Fail)]
pub enum OutputPointerParseError {
    #[fail(display = "Failed to parse transaction hash: {}", _0)]
    Hash(HashParseError),
    #[fail(
        display = "Output pointer has the wrong format, expected '<transaction id>:<output index>'"
    )]
    MissingColon,
    #[fail(display = "Could not parse output index as an integer: {}", _0)]
    ParseIntError(ParseIntError),
}

/// The error type for operations on a [`Secp256k1Signature`](Secp256k1Signature)
#[derive(Debug, PartialEq, Fail)]
pub enum Secp256k1ConversionError {
    #[fail(
        display = "Failed to convert `witnet_data_structures::Signature` into `secp256k1::Signature`"
    )]
    FailSignatureConversion,
    #[fail(
        display = "Failed to convert `witnet_data_structures::PublicKey` into `secp256k1::PublicKey`"
    )]
    FailPublicKeyConversion,
    #[fail(
        display = "Failed to convert `secp256k1::PublicKey` into `witnet_data_structures::PublicKey`: public key must be 33 bytes long, is {}",
        size
    )]
    FailPublicKeyFromSlice { size: usize },
    #[fail(
        display = "Failed to convert `witnet_data_structures::SecretKey` into `secp256k1::SecretKey`"
    )]
    FailSecretKeyConversion,
}

/// The error type for operations on a [`DataRequestPool`](DataRequestPool)
#[derive(Debug, PartialEq, Fail)]
pub enum DataRequestError {
    /// Add commit method failed.
    #[fail(
        display = "Block contains a commitment for an unknown data request:\n\
                   Block hash: {}\n\
                   Transaction hash: {}\n\
                   Data request: {}",
        block_hash, tx_hash, dr_pointer
    )]
    AddCommitFail {
        block_hash: Hash,
        tx_hash: Hash,
        dr_pointer: Hash,
    },
    /// Add reveal method failed.
    #[fail(
        display = "Block contains a reveal for an unknown data request:\n\
                   Block hash: {}\n\
                   Transaction hash: {}\n\
                   Data request: {}",
        block_hash, tx_hash, dr_pointer
    )]
    AddRevealFail {
        block_hash: Hash,
        tx_hash: Hash,
        dr_pointer: Hash,
    },
    /// Add tally method failed.
    #[fail(
        display = "Block contains a tally for an unknown data request:\n\
                   Block hash: {}\n\
                   Transaction hash: {}\n\
                   Data request: {}",
        block_hash, tx_hash, dr_pointer
    )]
    AddTallyFail {
        block_hash: Hash,
        tx_hash: Hash,
        dr_pointer: Hash,
    },
    #[fail(display = "Received a commitment and Data Request is not in Commit stage")]
    NotCommitStage,
    #[fail(display = "Received a reveal and Data Request is not in Reveal stage")]
    NotRevealStage,
    #[fail(display = "Received a tally and Data Request is not in Tally stage")]
    NotTallyStage,
    #[fail(display = "Cannot persist unfinished data request (with no Tally)")]
    UnfinishedDataRequest,
    #[fail(display = "The data request is not valid since it has no retrieval sources")]
    NoRetrievalSources,
    #[fail(display = "The data request has not a valid RadType")]
    InvalidRadType,
    /// Invalid fields in retrieval struct
    #[fail(
        display = "The retrieval has some fields that are not allowed for this retrieval kind ({:?}):\nexpected fields: {}\nactual fields: {}",
        kind, expected_fields, actual_fields
    )]
    MalformedRetrieval {
        kind: RADType,
        expected_fields: String,
        actual_fields: String,
    },
}

/// Possible errors when converting between epoch and timestamp
#[derive(Copy, Clone, Debug, PartialEq, Fail)]
pub enum EpochCalculationError {
    /// Checkpoint zero is in the future
    #[fail(display = "Checkpoint zero is in the future (timestamp: {})", _0)]
    CheckpointZeroInTheFuture(i64),
    /// Overflow when calculating the epoch timestamp
    #[fail(display = "Overflow when calculating the epoch timestamp")]
    Overflow,
}
