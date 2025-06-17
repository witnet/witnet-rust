use crate::staking::helpers::StakeKey;
use std::{
    convert::From,
    fmt::{Debug, Display},
    sync::PoisonError,
};
use thiserror::Error;

/// All errors related to the staking functionality.
#[derive(Debug, Eq, Error, PartialEq)]
pub enum StakesError<Address, Coins, Epoch>
where
    Address: Debug + Display + Sync + Send + 'static,
    Coins: Debug + Display + Sync + Send + 'static,
    Epoch: Debug + Display + Sync + Send + 'static,
{
    /// The amount of coins being staked or the amount that remains after unstaking is below the
    /// minimum stakeable amount.
    #[error(
        "The amount of coins being staked ({amount}) or the amount that remains after unstaking is below the minimum stakeable amount ({minimum})"
    )]
    AmountIsBelowMinimum {
        /// The number of coins being staked or remaining after staking.
        amount: Coins,
        /// The minimum stakeable amount.
        minimum: Coins,
    },
    /// Tried to query `Stakes` for information that belongs to the past.
    #[error(
        "Tried to query `Stakes` for information that belongs to the past. Query Epoch: {epoch} Latest Epoch: {latest}"
    )]
    EpochInThePast {
        ///  The Epoch being referred.
        epoch: Epoch,
        /// The latest Epoch.
        latest: Epoch,
    },
    /// An operation thrown an Epoch value that overflows.
    #[error(
        "An operation thrown an Epoch value that overflows. Computed Epoch: {computed} Maximum Epoch: {maximum}"
    )]
    EpochOverflow {
        /// The computed Epoch value.
        computed: u64,
        /// The maximum Epoch.
        maximum: Epoch,
    },
    /// Tried to query for a stake entry that is not registered in `Stakes`.
    #[error("Tried to query for a stake entry that is not registered in Stakes {key}")]
    EntryNotFound {
        /// A validator and withdrawer address pair.
        key: StakeKey<Address>,
    },
    /// Tried to obtain a lock on a write-locked piece of data that is already locked.
    #[error(
        "The authentication signature contained within a stake transaction is not valid for the given validator and withdrawer addresses"
    )]
    PoisonedLock,
    /// The authentication signature contained within a stake transaction is not valid for the given validator and
    /// withdrawer addresses.
    #[error(
        "The authentication signature contained within a stake transaction is not valid for the given validator and withdrawer addresses"
    )]
    InvalidAuthentication,
    /// Tried to query for a stake entry by validator that is not registered in `Stakes`.
    #[error(
        "Tried to query for a stake entry by validator ({validator}) that is not registered in Stakes"
    )]
    ValidatorNotFound {
        /// A validator address.
        validator: Address,
    },
    /// Tried to query for a stake entry by withdrawer that is not registered in `Stakes`.
    #[error(
        "Tried to query for a stake entry by withdrawer ({withdrawer}) that is not registered in Stakes"
    )]
    WithdrawerNotFound {
        /// A withdrawer address.
        withdrawer: Address,
    },
    /// Tried to add stake to a validator with a different withdrawer than the one initially set.
    #[error("Validator {validator} already has a different withdrawer set")]
    DifferentWithdrawer {
        /// A validator address.
        validator: Address,
    },
    /// Tried to query for a stake entry without providing a validator or a withdrawer address.
    #[error("Tried to query a stake entry without providing a validator or a withdrawer address")]
    EmptyQuery,
}

impl<T, Address, Coins, Epoch> From<PoisonError<T>> for StakesError<Address, Coins, Epoch>
where
    Address: Debug + Display + Sync + Send + 'static,
    Coins: Debug + Display + Sync + Send + 'static,
    Epoch: Debug + Display + Sync + Send + 'static,
{
    fn from(_value: PoisonError<T>) -> Self {
        StakesError::PoisonedLock
    }
}
