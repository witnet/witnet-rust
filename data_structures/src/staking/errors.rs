use std::sync::PoisonError;

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
    /// Tried to query `Stakes` for the address of a staker that is not registered in `Stakes`.
    IdentityNotFound {
        /// The unknown address.
        identity: Address,
    },
    /// Tried to obtain a lock on a write-locked piece of data that is already locked.
    PoisonedLock,
}

impl<T, Address, Coins, Epoch> From<PoisonError<T>> for StakesError<Address, Coins, Epoch> {
    fn from(_value: PoisonError<T>) -> Self {
        StakesError::PoisonedLock
    }
}
