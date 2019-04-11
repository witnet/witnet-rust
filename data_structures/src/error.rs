//! Error type definitions for the data structure module.

use failure::Fail;
use std::num::ParseIntError;

use super::chain::{Epoch, Hash, OutputPointer};

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
    /// The transaction creates value
    #[fail(display = "Transaction creates value (its fee is negative)")]
    NegativeFee,
    /// A transaction with the given hash wasn't found in a pool.
    #[fail(display = "A hash is missing in the pool (\"{}\")", hash)]
    PoolMiss { hash: Hash },
    /// An output with the given index wasn't found in a transaction.
    #[fail(display = "Output not found: {}", output)]
    OutputNotFound { output: OutputPointer },
    #[fail(display = "The transaction signature is invalid")]
    InvalidSignature,
    #[fail(display = "Mint transaction is invalid")]
    InvalidMintTransaction,
    #[fail(display = "Data Request transaction is invalid")]
    InvalidDataRequestTransaction,
    #[fail(display = "Commit transaction is invalid")]
    InvalidCommitTransaction,
    #[fail(display = "Reveal transaction is invalid")]
    InvalidRevealTransaction,
    #[fail(display = "Tally transaction is invalid")]
    InvalidTallyTransaction,
    #[fail(display = "Commit transaction has not a DataRequest Input")]
    NotDataRequestInputInCommit,
    #[fail(display = "Reveal transaction has not a Commit Input")]
    NotCommitInputInReveal,
    #[fail(display = "Tally transaction has not a Reveal Input")]
    NotRevealInputInTally,
    #[fail(display = "Commit transaction has a invalid Proof of Eligibility")]
    InvalidDataRequestPoe,
    #[fail(display = "Invalid fee found: {}. Expected fee: {}", fee, expected_fee)]
    InvalidFee { fee: u64, expected_fee: u64 },
    #[fail(display = "Invalid Data Request reward: {}", reward)]
    InvalidDataRequestReward { reward: i64 },
    #[fail(
        display = "Invalid Data Request reward ({}) for this number of witnesses ({})",
        dr_value, witnesses
    )]
    InvalidDataRequestValue { dr_value: i64, witnesses: i64 },
    #[fail(display = "Data Request witnesses number is not enough")]
    InsufficientWitnesses,
    #[fail(display = "Reveals from different Data Requests")]
    RevealsFromDifferentDataRequest,
    #[fail(
        display = "Mismatching between local tally ({:?}) and miner tally ({:?})",
        local_tally, miner_tally
    )]
    MismatchedConsensus {
        local_tally: Vec<u8>,
        miner_tally: Vec<u8>,
    },
    #[fail(
        display = "Mismatching number of signatures ({}) and inputs ({})",
        signatures_n, inputs_n
    )]
    MismatchingSignaturesNumber { signatures_n: u8, inputs_n: u8 },
    #[fail(display = "Invalid Position for a Mint Transaction")]
    UnexpectedMint,
    /// Transaction verification process failed.
    #[fail(
        display = "Failed to verify the signature at index {} in transaction {}",
        index, hash
    )]
    VerifyTransactionSignatureFail { hash: Hash, index: u8 },
    /// Signature not found
    #[fail(display = "Transaction signature not found")]
    SignatureNotFound,
}

/// The error type for operations on a [`Block`](Block)
#[derive(Debug, PartialEq, Fail)]
pub enum BlockError {
    /// The block has no transactions in it.
    #[fail(display = "The block has no transactions")]
    Empty,
    /// The first transaction of the block is no mint.
    #[fail(display = "The block first transaction is not a mint transactions")]
    NoMint,
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
    #[fail(display = "The block has an invalid PoE")]
    NotValidPoe,
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
        display = "Ignoring block because previous hash (\"{}\") is unknown",
        hash
    )]
    PreviousHashNotKnown { hash: Hash },
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
}

#[derive(Debug, Fail)]
pub enum OutputPointerParseError {
    #[fail(display = "output pointer has an invalid length")]
    InvalidHashLength,
    #[fail(
    display = "output pointer has the wrong format, expected '<transaction id>:<output index>'"
    )]
    MissingColon,
    #[fail(display = "could not parse output index as an integer")]
    ParseIntError(ParseIntError),
}