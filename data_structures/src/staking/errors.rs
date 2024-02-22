use std::sync::PoisonError;

use crate::staking::aux::StakeKey;

/// All errors related to the staking functionality.
#[derive(Debug, PartialEq)]
pub enum StakesError<Address, Coins, Epoch> {
    /// The amount of coins being staked or the amount that remains after unstaking is below the
    /// minimum stakeable amount.
    AmountIsBelowMinimum {
        /// The number of coins being staked or remaining after staking.
        amount: Coins,
        /// The minimum stakeable amount.
        minimum: Coins,
    },
    /// Tried to query `Stakes` for information that belongs to the past.
    EpochInThePast {
        ///  The Epoch being referred.
        epoch: Epoch,
        /// The latest Epoch.
        latest: Epoch,
    },
    /// An operation thrown an Epoch value that overflows.
    EpochOverflow {
        /// The computed Epoch value.
        computed: u64,
        /// The maximum Epoch.
        maximum: Epoch,
    },
    /// Tried to query for a stake entry that is not registered in `Stakes`.
    EntryNotFound {
        /// A validator and withdrawer address pair.
        key: StakeKey<Address>,
    },
    /// Tried to obtain a lock on a write-locked piece of data that is already locked.
    PoisonedLock,
    /// The authentication signature contained within a stake transaction is not valid for the given validator and
    /// withdrawer addresses.
    InvalidAuthentication,
}

impl<T, Address, Coins, Epoch> From<PoisonError<T>> for StakesError<Address, Coins, Epoch> {
    fn from(_value: PoisonError<T>) -> Self {
        StakesError::PoisonedLock
    }
}
