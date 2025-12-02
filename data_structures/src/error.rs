//! Error type definitions for the data structure module.

use hex::FromHexError;
use std::num::ParseIntError;
use thiserror::Error;
use witnet_crypto::secp256k1;

use crate::chain::{
    DataRequestOutput, Epoch, Hash, HashParseError, OutputPointer, PublicKeyHash, RADType,
};

/// The error type for operations on a [`ChainInfo`](ChainInfo)
#[derive(Debug, PartialEq, Eq, Error)]
pub enum ChainInfoError {
    /// Errors when try to use a None value for ChainInfo
    #[error("No ChainInfo loaded in ChainManager")]
    ChainInfoNotFound,
}

/// Error in builders functions
#[derive(Debug, PartialEq, Eq, Error)]
pub enum BuildersError {
    /// No inventory vectors available to create a Inventory Announcement message
    #[error("No inventory vectors available to create a Inventory Announcement message")]
    NoInvVectorsAnnouncement,
    /// No inventory vectors available to create a Inventory Request message
    #[error("No inventory vectors available to create a Inventory Request message")]
    NoInvVectorsRequest,
}

/// The error type for operations on a [`Transaction`](Transaction)
#[derive(Debug, PartialEq, Eq, Error)]
pub enum TransactionError {
    #[error("The transaction is invalid")]
    NotValidTransaction,
    #[error("Sum of fees overflows")]
    FeeOverflow,
    #[error("Sum of input values overflows")]
    InputValueOverflow,
    #[error("Sum of output values overflows")]
    OutputValueOverflow,
    /// The transaction creates value
    #[error("Transaction creates value (its fee is negative)")]
    NegativeFee,
    /// An output with the given index wasn't found in a transaction.
    #[error("Output not found: {output}")]
    OutputNotFound { output: OutputPointer },
    #[error("Data Request not found: {hash}")]
    DataRequestNotFound { hash: Hash },
    #[error("Too many witnesses data request not found in the same block: {hash}")]
    TooManyWitnessesDataRequestNotFound { hash: Hash },
    #[error("Commit transaction has an invalid Proof of Eligibility")]
    InvalidDataRequestPoe,
    #[error("Validator {validator} is not eligible to commit to a data request")]
    ValidatorNotEligible { validator: PublicKeyHash },
    #[error(
        "The data request eligibility claim VRF proof hash is greater than the target hash: {vrf_hash} > {target_hash}"
    )]
    DataRequestEligibilityDoesNotMeetTarget { vrf_hash: Hash, target_hash: Hash },
    #[error("Invalid tally change found: {change}. Expected value: {expected_change}")]
    InvalidTallyChange { change: u64, expected_change: u64 },
    #[error("Invalid witness reward found: {value}. Expected value: {expected_value}")]
    InvalidReward { value: u64, expected_value: u64 },
    #[error(
        "In tally validation, the total amount is incorrect. Found: {value}. Expected value: {expected_value}"
    )]
    InvalidTallyValue { value: u64, expected_value: u64 },
    #[error("Data Request witness reward must be greater than zero")]
    NoReward,
    #[error("Data Request witnesses number must be greater than zero")]
    InsufficientWitnesses,
    #[error(
        "Mismatch between expected tally ({expected_tally:?}) and miner tally ({miner_tally:?})"
    )]
    MismatchedConsensus {
        expected_tally: Vec<u8>,
        miner_tally: Vec<u8>,
    },
    #[error("Mismatching number of signatures ({signatures_n}) and inputs ({inputs_n})")]
    MismatchingSignaturesNumber { signatures_n: u8, inputs_n: u8 },
    /// Transaction verification process failed.
    #[error("Failed to verify the signature of transaction {hash}: {msg}")]
    VerifyTransactionSignatureFail { hash: Hash, msg: String },
    /// Signature not found
    #[error("Transaction signature not found")]
    SignatureNotFound,
    /// Public Key Hash does not match
    #[error("Public key hash mismatch: expected {expected_pkh} got {signature_pkh}")]
    PublicKeyHashMismatch {
        expected_pkh: PublicKeyHash,
        signature_pkh: PublicKeyHash,
    },
    /// Commit related to a reveal not found
    #[error("Commitment related to a reveal not found")]
    CommitNotFound,
    /// Reveal related to a tally not found
    #[error("Reveal related to a tally not found")]
    RevealNotFound,
    /// Commitment field in CommitTransaction does not match with RevealTransaction signature
    #[error(
        "Commitment field in CommitTransaction does not match with RevealTransaction signature"
    )]
    MismatchedCommitment,
    /// No inputs when the transaction must have at least one
    #[error("Transaction {tx_hash} cannot have zero inputs")]
    NoInputs { tx_hash: Hash },
    #[error("Genesis transaction should have 0 inputs, but has {inputs_n} inputs")]
    InputsInGenesis { inputs_n: usize },
    #[error("Genesis transactions cannot have 0 outputs")]
    NoOutputsInGenesis,
    /// An output with zero value does not make sense
    #[error("Transaction {tx_hash} has a zero value output at index {output_id}")]
    ZeroValueOutput { tx_hash: Hash, output_id: usize },
    /// A dishonest witness has been rewarded
    #[error("A dishonest witness has been rewarded")]
    DishonestReward,
    /// This pkh was rewarded previously
    #[error("This pkh {pkh} was rewarded previously")]
    MultipleRewards { pkh: PublicKeyHash },
    /// There are a different number of outputs than expected
    #[error(
        "There are a different number of outputs ({outputs}) than expected ({expected_outputs})"
    )]
    WrongNumberOutputs {
        outputs: usize,
        expected_outputs: usize,
    },
    /// Transaction has a time lock and cannot be included in this epoch
    #[error(
        "Transaction cannot be included before {expected} but the block timestamp is {current}"
    )]
    TimeLock { current: i64, expected: i64 },
    /// Value Transfer Output has an invalid time lock
    #[error("Value Transfer Output time_lock should be {expected}, but it is {current}")]
    InvalidTimeLock { current: u64, expected: u64 },
    /// This commit was already included
    #[error("Commit with pkh {pkh} was already included for the data request {dr_pointer}")]
    DuplicatedCommit {
        pkh: PublicKeyHash,
        dr_pointer: Hash,
    },
    /// This reveal was already included
    #[error("Reveal with pkh {pkh} was already included for the data request {dr_pointer}")]
    DuplicatedReveal {
        pkh: PublicKeyHash,
        dr_pointer: Hash,
    },
    /// This tally was already included
    #[error("Tally was already included for the data request {dr_pointer}")]
    DuplicatedTally { dr_pointer: Hash },
    /// RadonReport not in Tally Stage
    #[error("RadonReport not in Tally Stage")]
    NoTallyStage,
    /// Minimum consensus percentage is invalid
    #[error("Minimum consensus percentage {value} is invalid. Must be >50 and <100")]
    InvalidMinConsensus { value: u32 },
    /// Error when there is not enough balance to create a transaction
    #[error(
        "Cannot build a transaction. Transaction value is greater than available balance: \
             (Total Balance: {total_balance}, Available Balance: {available_balance}, Transaction value: {transaction_value})"
    )]
    NoMoney {
        total_balance: u64,
        available_balance: u64,
        transaction_value: u64,
    },
    /// Zero amount specified
    #[error("A transaction with zero value is invalid")]
    ZeroAmount,
    /// Incorrect count of out-of-consensus witnesses in Tally
    #[error(
        "Incorrect count of out-of-consensus witnesses in Tally. Expected: {expected:?}, found: {found:?}"
    )]
    MismatchingOutOfConsensusCount {
        expected: Vec<PublicKeyHash>,
        found: Vec<PublicKeyHash>,
    },
    /// Incorrect count of witnesses with errors in Tally
    #[error(
        "Incorrect count of witnesses with errors in Tally. Expected: {expected:?}, found: {found:?}"
    )]
    MismatchingErrorCount {
        expected: Vec<PublicKeyHash>,
        found: Vec<PublicKeyHash>,
    },
    /// Invalid collateral in data request
    #[error(
        "The specified collateral ({value} nwits), is less than the minimum required {min} nwits)"
    )]
    InvalidCollateral { value: u64, min: u64 },
    /// Negative collateral in commit transaction
    #[error(
        "Negative collateral in commit transaction. Input value: {input_value}, output value: {output_value}"
    )]
    NegativeCollateral { input_value: u64, output_value: u64 },
    /// Incorrect collateral in commit transaction
    #[error("Incorrect collateral. Expected: {expected}, found: {found}")]
    IncorrectCollateral { expected: u64, found: u64 },
    /// Collateral in commit transaction is not mature enough
    #[error(
        "Output {output} used as input for collateralized commitment is not mature enough. Inputs of commitment transactions must be older than {must_be_older_than} blocks, but this one was only {found} blocks old"
    )]
    CollateralNotMature {
        must_be_older_than: u32,
        found: u32,
        output: OutputPointer,
    },
    /// Collateral in commit transaction uses a different PKH than the commit VRF Proof
    #[error(
        "Output {output} used as input for collateralized commitment has pkh {output_pkh} when the commit proof has pkh {proof_pkh}"
    )]
    CollateralPkhMismatch {
        output: OutputPointer,
        output_pkh: PublicKeyHash,
        proof_pkh: PublicKeyHash,
    },
    /// The committer does not satisfy the qualification requirements introduced for the 2.0 transition
    #[error(
        "Unqualified committer: {committer}. Required balance: {required}, current balance: {current}"
    )]
    UnqualifiedCommitter {
        committer: PublicKeyHash,
        required: u64,
        current: u64,
    },
    /// More than one output for the collateral change
    #[error("More than one output for the collateral change")]
    SeveralCommitOutputs,
    /// Value Transfer weight limit exceeded
    #[error("Value Transfer Transaction weight ({weight}) exceeds the limit {max_weight})")]
    ValueTransferWeightLimitExceeded { weight: u32, max_weight: u32 },
    /// Data Request weight limit exceeded
    #[error(
        "Data Request Transaction weight ({weight}) exceeds the limit {max_weight})\n > {dr_output:?}"
    )]
    DataRequestWeightLimitExceeded {
        weight: u32,
        max_weight: u32,
        dr_output: Box<DataRequestOutput>,
    },
    /// Stake amount below minimum
    #[error("The amount of coins in stake ({stake}) is less than the minimum allowed {min_stake})")]
    StakeBelowMinimum { min_stake: u64, stake: u64 },
    /// Stake amount above maximum
    #[error("The amount of coins in stake ({stake}) is more than the maximum allowed {max_stake})")]
    StakeAboveMaximum { max_stake: u64, stake: u64 },
    /// Stake weight limit exceeded
    #[error("Stake Transaction weight ({weight}) exceeds the limit {max_weight})")]
    StakeWeightLimitExceeded { weight: u32, max_weight: u32 },
    /// Unstaking more than the total staked
    #[error("Tried to unstake more coins than the current stake ({unstake} > {stake})")]
    UnstakingMoreThanStaked { stake: u64, unstake: u64 },
    /// Tried to perform an unstake action with an invalid nonce.
    #[error("Cannot unstake with an invalid nonce: {used} < {current}")]
    UnstakeInvalidNonce { used: u64, current: u64 },
    /// An stake output with zero value does not make sense
    #[error("Transaction {tx_hash} has a zero value stake output")]
    ZeroValueStakeOutput { tx_hash: Hash },
    /// No stake transactions allowed yet
    #[error("No stake transactions allowed yet")]
    NoStakeTransactionsAllowed,
    /// Invalid unstake signature
    #[error(
        "Invalid unstake signature: ({signature}), withdrawal ({withdrawal}), operator {operator})"
    )]
    InvalidUnstakeSignature {
        signature: PublicKeyHash,
        withdrawal: PublicKeyHash,
        operator: PublicKeyHash,
    },
    /// Invalid unstake time_lock
    #[error(
        "The unstake timelock: ({time_lock}) is lower than the minimum unstaking delay {unstaking_delay_seconds})"
    )]
    InvalidUnstakeTimelock {
        time_lock: u64,
        unstaking_delay_seconds: u64,
    },
    /// Invalid unstake request
    #[error("No stake found for validator and withdrawer pair ({validator}, {withdrawer})")]
    NoStakeFound {
        validator: PublicKeyHash,
        withdrawer: PublicKeyHash,
    },
    /// The collateral requirement would reduce the validator's balance below the minimum required stake amount
    #[error(
        "Collateral requirement of {collateral} would put validator {validator} stake below the minimum stake amount"
    )]
    CollateralBelowMinimumStake {
        collateral: u64,
        validator: PublicKeyHash,
    },
    /// No unstake transactions allowed yet
    #[error("No unstake transactions allowed yet")]
    NoUnstakeTransactionsAllowed,
    #[error(
        "The reward-to-collateral ratio for this data request is {reward_collateral_ratio}, but must be equal or less than {required_reward_collateral_ratio}"
    )]
    RewardTooLow {
        reward_collateral_ratio: u64,
        required_reward_collateral_ratio: u64,
    },
}

/// The error type for operations on a [`Block`](Block)
#[derive(Debug, PartialEq, Eq, Error)]
pub enum BlockError {
    /// The block has no transactions in it.
    #[error("The block has no transactions")]
    Empty,
    /// The total value created by the mint transaction of the block,
    /// and the output value of the rest of the transactions, plus the
    /// block reward, don't add up
    #[error(
        "The value of the mint transaction does not match the fees + reward of the block ({mint_value} != {fees_value} + {reward_value})"
    )]
    MismatchedMintValue {
        mint_value: u64,
        fees_value: u64,
        reward_value: u64,
    },
    #[error("MintTransaction was split in more than two 'ValueTransferOutput'")]
    TooSplitMint,
    #[error("Mint transaction has invalid epoch: mint {mint_epoch}, block {block_epoch}")]
    InvalidMintEpoch {
        mint_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[error("Mint transaction should be set to default after the activation of Witnet 2.0")]
    InvalidMintTransaction,
    #[error("The block has an invalid PoE")]
    NotValidPoe,
    #[error(
        "The block eligibility claim VRF proof hash is greater than the target hash: {vrf_hash} > {target_hash}"
    )]
    BlockEligibilityDoesNotMeetTarget { vrf_hash: Hash, target_hash: Hash },
    #[error("The block has an invalid Merkle Tree")]
    NotValidMerkleTree,
    #[error(
        "Block epoch from the future. Current epoch is: {current_epoch}, block epoch is: {block_epoch}"
    )]
    BlockFromFuture {
        current_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[error(
        "Ignoring block because its epoch ({block_epoch}) is older than highest block checkpoint {chain_epoch})"
    )]
    BlockOlderThanTip {
        chain_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[error(
        "Ignoring block because previous hash (\"{block_hash}\") is different from our top block hash (\"{our_hash}\")"
    )]
    PreviousHashMismatch { block_hash: Hash, our_hash: Hash },
    #[error(
        "Ignoring genesis block because it is different from our expected genesis block:\nBlock:    `{block}`\nExpected: `{expected}`"
    )]
    GenesisBlockMismatch { block: String, expected: String },
    #[error(
        "Ignoring genesis block because its hash (\"{block_hash}\") is different from our expected genesis block hash (\"{expected_hash}\")"
    )]
    GenesisBlockHashMismatch {
        block_hash: Hash,
        expected_hash: Hash,
    },
    #[error(
        "Genesis block creates more value than allowed. Value cannot be greater than {max_total_value}"
    )]
    GenesisValueOverflow { max_total_value: u64 },
    #[error(
        "Block candidate's epoch differs from current epoch ({block_epoch} != {current_epoch})"
    )]
    CandidateFromDifferentEpoch {
        current_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[error("Commits in block ({commits}) are not equal to commits required {rf})")]
    MismatchingCommitsNumber { commits: u32, rf: u32 },
    /// Block verification signature process failed.
    #[error("Failed to verify the signature of block {hash}")]
    VerifySignatureFail { hash: Hash },
    /// Public Key Hash does not match
    #[error("Public key hash mismatch: VRF Proof PKH: {proof_pkh}, signature PKH: {signature_pkh}")]
    PublicKeyHashMismatch {
        proof_pkh: PublicKeyHash,
        signature_pkh: PublicKeyHash,
    },
    /// Value Transfer weight limit exceeded
    #[error(
        "Total weight of Value Transfer Transactions in a block ({weight}) exceeds the limit {max_weight})"
    )]
    TotalValueTransferWeightLimitExceeded { weight: u32, max_weight: u32 },
    /// Data Request weight limit exceeded
    #[error(
        "Total weight of Data Request Transactions in a block ({weight}) exceeds the limit {max_weight})"
    )]
    TotalDataRequestWeightLimitExceeded { weight: u32, max_weight: u32 },
    /// Stake weight limit exceeded by a block candidate
    #[error(
        "Total weight of Stake Transactions in a block ({weight}) exceeds the limit {max_weight})"
    )]
    TotalStakeWeightLimitExceeded { weight: u32, max_weight: u32 },
    /// Unstake weight limit exceeded
    #[error(
        "Total weight of Unstake Transactions in a block ({weight}) exceeds the limit {max_weight})"
    )]
    TotalUnstakeWeightLimitExceeded { weight: u32, max_weight: u32 },
    /// Repeated operator Stake
    #[error("A single operator is receiving stake more than once in a block: ({pkh})")]
    RepeatedStakeOperator { pkh: PublicKeyHash },
    /// Repeated operator Unstake
    #[error("A single operator is withdrawing stake more than once in a block: ({pkh})")]
    RepeatedUnstakeOperator { pkh: PublicKeyHash },
    /// Missing expected tallies
    #[error("{count} expected tally transactions are missing in block candidate {block_hash}")]
    MissingExpectedTallies { count: usize, block_hash: Hash },
    /// Validator is not eligible to propose a block
    #[error("Validator {validator} is not eligible to propose a block")]
    ValidatorNotEligible { validator: PublicKeyHash },
}

#[derive(Debug, Error)]
pub enum OutputPointerParseError {
    #[error("Failed to parse transaction hash: {0}")]
    Hash(HashParseError),
    #[error("Output pointer has the wrong format, expected '<transaction id>:<output index>'")]
    MissingColon,
    #[error("Could not parse output index as an integer: {0}")]
    ParseIntError(ParseIntError),
}

/// The error type for operations on a [`Secp256k1Signature`](Secp256k1Signature)
#[derive(Debug, PartialEq, Error)]
pub enum Secp256k1ConversionError {
    #[error("Failed to convert `witnet_data_structures::Signature` into `secp256k1::Signature`")]
    FailSignatureConversion,
    #[error("Failed to convert `witnet_data_structures::PublicKey` into `secp256k1::PublicKey`")]
    FailPublicKeyConversion,
    #[error(
        "Failed to convert `secp256k1::PublicKey` into `witnet_data_structures::PublicKey`: public key must be 33 bytes long, is {size}"
    )]
    FailPublicKeyFromSlice { size: usize },
    #[error("Failed to convert `witnet_data_structures::SecretKey` into `secp256k1::SecretKey`")]
    FailSecretKeyConversion,
    #[error(
        "Cannot decode a `witnet_data_structures::KeyedSignature` from the allegedly hex-encoded string '{hex}': {inner}"
    )]
    HexDecode { hex: String, inner: FromHexError },
    #[error("{inner}")]
    Secp256k1 { inner: secp256k1::Error },
    #[error("{inner}")]
    Other { inner: String },
}

/// The error type for operations on a [`DataRequestPool`](DataRequestPool)
#[derive(Debug, PartialEq, Eq, Error)]
pub enum DataRequestError {
    /// Add commit method failed.
    #[error(
        "Block contains a commitment for an unknown data request:\n\
             Block hash: {block_hash}\n\
             Transaction hash: {tx_hash}\n\
             Data request: {dr_pointer}"
    )]
    AddCommitFail {
        block_hash: Hash,
        tx_hash: Hash,
        dr_pointer: Hash,
    },
    /// Add reveal method failed.
    #[error(
        "Block contains a reveal for an unknown data request:\n\
             Block hash: {block_hash}\n\
             Transaction hash: {tx_hash}\n\
             Data request: {dr_pointer}"
    )]
    AddRevealFail {
        block_hash: Hash,
        tx_hash: Hash,
        dr_pointer: Hash,
    },
    /// Add tally method failed.
    #[error(
        "Block contains a tally for an unknown data request:\n\
             Block hash: {block_hash}\n\
             Transaction hash: {tx_hash}\n\
             Data request: {dr_pointer}"
    )]
    AddTallyFail {
        block_hash: Hash,
        tx_hash: Hash,
        dr_pointer: Hash,
    },
    #[error("API key {api_key} required to solve this data request was not found")]
    NoApiKeyFound { api_key: String },
    #[error("Received a commitment and Data Request is not in Commit stage")]
    NotCommitStage,
    #[error("Received a reveal and Data Request is not in Reveal stage")]
    NotRevealStage,
    #[error("Received a tally and Data Request is not in Tally stage")]
    NotTallyStage,
    #[error("No API key found in the URL or headers")]
    RequestWithoutApiKey,
    #[error("Cannot persist unfinished data request (with no Tally)")]
    UnfinishedDataRequest,
    #[error("The data request is not valid since it has no retrieval sources")]
    NoRetrievalSources,
    #[error("The data request has not a valid RadType")]
    InvalidRadType,
    /// Invalid fields in retrieval struct
    #[error(
        "The retrieval has some fields that are not allowed for this retrieval kind ({kind:?}):\nexpected fields: {expected_fields}\nactual fields: {actual_fields}"
    )]
    MalformedRetrieval {
        kind: RADType,
        expected_fields: String,
        actual_fields: String,
    },
}

/// Possible errors when converting between epoch and timestamp
#[derive(Copy, Clone, Debug, PartialEq, Eq, Error)]
pub enum EpochCalculationError {
    /// Checkpoint zero is in the future
    #[error("Checkpoint zero is in the future (timestamp: {0})")]
    CheckpointZeroInTheFuture(i64),
    /// Overflow when calculating the epoch timestamp
    #[error("Overflow when calculating the epoch timestamp")]
    Overflow,
}
