use std::fmt::{Debug, Display};
use std::{marker::PhantomData, ops::*};

use serde::{Deserialize, Serialize};

use crate::wit::PrecisionLoss;

use super::prelude::*;

/// A data structure that keeps track of a staker's staked coins and the epochs for different
/// capabilities.
#[derive(Copy, Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Stake<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default,
    Coins: Clone,
    Epoch: Clone + Default,
    Nonce: Clone + Default,
    Power: Clone,
{
    /// An amount of staked coins.
    pub coins: Coins,
    /// The average epoch used to derive coin age for different capabilities.
    pub epochs: CapabilityMap<Epoch>,
    /// The amount of stake and unstake actions.
    pub nonce: Nonce,
    /// This phantom field is here just for the sake of specifying generics.
    #[serde(skip)]
    pub phantom_address: PhantomData<Address>,
    /// This phantom field is here just for the sake of specifying generics.
    #[serde(skip)]
    pub phantom_power: PhantomData<Power>,
}

impl<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
    Stake<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default + Debug + Display + Sync + Send,
    Coins: Copy
        + Clone
        + From<u64>
        + PartialOrd
        + num_traits::Zero
        + Add<Output = Coins>
        + Sub<Output = Coins>
        + Mul
        + Mul<Epoch, Output = Power>
        + Debug
        + Display
        + Send
        + Sync
        + PrecisionLoss,
    Epoch: Copy
        + Clone
        + Default
        + num_traits::Saturating
        + Sub<Output = Epoch>
        + From<u32>
        + Debug
        + Display
        + Sync
        + Send,
    Nonce: Copy
        + Clone
        + Default
        + num_traits::Saturating
        + AddAssign
        + From<u32>
        + Debug
        + Display
        + Sync
        + Send,
    Power: Add<Output = Power> + Clone + Div<Output = Power>,
    u64: From<Coins> + From<Power>,
{
    /// Increase the amount of coins staked by a certain staker.
    ///
    /// When adding stake:
    /// - Amounts are added together.
    /// - Epochs are weight-averaged, using the amounts as the weight.
    ///
    /// This type of averaging makes the entry equivalent to an unbounded record of all stake
    /// additions and removals, without the overhead in memory and computation.
    pub fn add_stake(
        &mut self,
        coins: Coins,
        epoch: Epoch,
        increment_nonce: bool,
        minimum_stakeable: Coins,
    ) -> StakesResult<Coins, Address, Coins, Epoch> {
        // Make sure that the amount to be staked is equal or greater than the minimum
        if coins < minimum_stakeable {
            Err(StakesError::AmountIsBelowMinimum {
                amount: coins,
                minimum: minimum_stakeable,
            })?;
        }

        let coins_before = self.coins;
        let coins_after = coins_before + coins;
        self.coins = coins_after;

        // When stake is added, all capabilities get their associated epochs moved to the past
        for capability in ALL_CAPABILITIES {
            let epoch_before = self.epochs.get(capability);
            let product_before = coins_before.lose_precision(UNIT) * epoch_before;
            let product_added = coins.lose_precision(UNIT) * epoch;

            #[allow(clippy::cast_possible_truncation)]
            let epoch_after = Epoch::from(
                (u64::from(product_before + product_added)
                    / u64::from(coins_after.lose_precision(UNIT))) as u32,
            );
            self.epochs.update(capability, epoch_after);
        }

        if increment_nonce {
            self.nonce += Nonce::from(1);
        }

        Ok(coins_after)
    }

    /// Construct a Stake entry from a number of coins and a capability map. This is only useful for
    /// tests.
    #[cfg(test)]
    pub fn from_parts(coins: Coins, epochs: CapabilityMap<Epoch>, nonce: Nonce) -> Self {
        Self {
            coins,
            epochs,
            nonce,
            phantom_address: Default::default(),
            phantom_power: Default::default(),
        }
    }

    /// Derives the power of an identity in the network on a certain epoch from an entry. Most
    /// normally, the epoch is the current epoch.
    pub fn power(&self, capability: Capability, current_epoch: Epoch) -> Power {
        self.coins.lose_precision(UNIT)
            * (current_epoch.saturating_sub(self.epochs.get(capability)))
    }

    /// Remove a certain amount of staked coins.
    pub fn remove_stake(
        &mut self,
        coins: Coins,
        increment_nonce: bool,
        minimum_stakeable: Coins,
    ) -> StakesResult<Coins, Address, Coins, Epoch> {
        let coins_after = self.coins.sub(coins);

        if coins_after > Coins::zero() && coins_after < minimum_stakeable {
            Err(StakesError::AmountIsBelowMinimum {
                amount: coins_after,
                minimum: minimum_stakeable,
            })?;
        }

        self.coins = coins_after;

        if increment_nonce {
            self.nonce += Nonce::from(1);
        }

        Ok(self.coins)
    }

    /// Set the epoch for a certain capability. Most normally, the epoch is the current epoch.
    pub fn reset_age(&mut self, capability: Capability, reset_epoch: Epoch) {
        self.epochs.update(capability, reset_epoch);
    }
}

/// Adds up the total amount of staked in multiple stake entries provided as a vector.
pub fn totalize_stakes<const UNIT: u8, Address, Coins, Epoch, Nonce, I, Power, S>(
    stakes: I,
) -> StakesResult<Coins, Address, Coins, Epoch>
where
    Address: Clone + Debug + Default + Display + Send + Sync,
    Coins: Clone + Debug + Display + num_traits::Zero + Send + Sync,
    Epoch: Clone + Debug + Default + Display + Send + Sync,
    Nonce: Clone + Debug + Default + Display + Send + Sync,
    I: IntoIterator<Item = S>,
    Power: Clone,
    S: Into<Stake<UNIT, Address, Coins, Epoch, Nonce, Power>>,
{
    Ok(stakes
        .into_iter()
        .fold(Coins::zero(), |a: Coins, b| a + b.into().coins))
}
