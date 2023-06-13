use num_traits::Zero;
use std::collections::BTreeMap;

use crate::wit::NANOWITS_PER_WIT;
use crate::{
    chain::{Epoch, PublicKeyHash},
    wit::Wit,
};

/// A minimum stakeable amount needs to exist to prevent spamming of the tracker.
const MINIMUM_STAKEABLE_AMOUNT_WITS: u64 = 10;
/// A maximum coin age is enforced to prevent an actor from monopolizing eligibility by means of
/// hoarding coin age.
const MAXIMUM_COIN_AGE_EPOCHS: u64 = 53_760;

/// Type alias that represents the power of an identity in the network on a certain epoch.
///
/// This is expected to be used for deriving eligibility.
pub type Power = u64;

#[derive(Debug, PartialEq)]
pub enum StakesTrackerError {
    AmountIsBelowMinimum { amount: Wit, minimum: Wit },
    EpochInThePast { epoch: Epoch, latest: Epoch },
    EpochOverflow { computed: u64, maximum: Epoch },
    IdentityNotFound { identity: PublicKeyHash },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StakesEntry {
    /// How many coins does an identity have in stake
    coins: Wit,
    /// The weighted average of the epochs in which the stake was added
    epoch: Epoch,
    /// Further entries representing coins that are queued for unstaking
    exiting_coins: Vec<Box<StakesEntry>>,
}

impl StakesEntry {
    /// Updates an entry for a given epoch with a certain amount of coins.
    ///
    /// - Amounts are added together.
    /// - Epochs are weight-averaged, using the amounts as the weight.
    ///
    /// This type of averaging makes the entry equivalent to an unbounded record of all stake
    /// additions and removals, without the overhead in memory and computation.
    pub fn add_stake(
        &mut self,
        amount: Wit,
        epoch: Epoch,
    ) -> Result<&StakesEntry, StakesTrackerError> {
        // Make sure that the amount to be staked is equal or greater than the minimum
        let minimum = Wit::from_wits(MINIMUM_STAKEABLE_AMOUNT_WITS);
        if amount < minimum {
            return Err(StakesTrackerError::AmountIsBelowMinimum { amount, minimum });
        }

        let coins_before = self.coins;
        let epoch_before = self.epoch;

        // These "products" simply use the staked amount as the weight for the weighted average
        let product_before = coins_before.nanowits() * u64::from(epoch_before);
        let product_added = amount.nanowits() * u64::from(epoch);

        let coins_after = coins_before + amount;
        let epoch_after = (product_before + product_added) / coins_after.nanowits();

        self.coins = coins_after;
        self.epoch =
            Epoch::try_from(epoch_after).map_err(|_| StakesTrackerError::EpochOverflow {
                computed: epoch_after,
                maximum: Epoch::MAX,
            })?;

        return Ok(self);
    }

    /// Derives the power of an identity in the network on a certain epoch from an entry.
    ///
    /// A cap on coin age is enforced, and thus the maximum power is the total supply multiplied by
    /// that cap.
    pub fn power(&self, epoch: Epoch) -> Power {
        let age = u64::from(epoch.saturating_sub(self.epoch)).min(MAXIMUM_COIN_AGE_EPOCHS);
        let nano_wits = self.coins.nanowits();
        let power = nano_wits.saturating_mul(age) / NANOWITS_PER_WIT;

        power
    }

    /// Remove a certain amount of staked coins.
    pub fn remove_stake(&mut self, amount: Wit) -> Result<&StakesEntry, StakesTrackerError> {
        // Make sure that the amount left in staked is equal or greater than the minimum
        let minimum = Wit::from_wits(MINIMUM_STAKEABLE_AMOUNT_WITS);
        let coins_after =
            Wit::from_nanowits(self.coins.nanowits().saturating_sub(amount.nanowits()));
        if coins_after > Wit::zero() && coins_after < minimum {
            return Err(StakesTrackerError::AmountIsBelowMinimum { amount, minimum });
        }

        self.coins = coins_after;

        return Ok(self);
    }
}

/// Accumulates global stats about the staking tracker.
#[derive(Debug, Default, PartialEq)]
pub struct StakingStats {
    /// Represents the average amount and epoch of the staked coins.
    pub average: StakesEntry,
    /// The latest epoch for which there is information in the tracker.
    pub latest_epoch: Epoch,
}

#[derive(Default)]
pub struct StakesTracker {
    /// The individual stake records for all identities with a non-zero stake.
    entries: BTreeMap<PublicKeyHash, StakesEntry>,
    /// Accumulates global stats about the staking tracker, as derived from the entries.
    stats: StakingStats,
}

impl StakesTracker {
    /// Register a certain amount of additional stake for a certain identity and epoch.
    pub fn add_stake(
        &mut self,
        identity: &PublicKeyHash,
        amount: Wit,
        epoch: Epoch,
    ) -> Result<&StakesEntry, StakesTrackerError> {
        // Refuse to add a stake for an epoch in the past
        let latest = self.stats.latest_epoch;
        if epoch < latest {
            return Err(StakesTrackerError::EpochInThePast { epoch, latest });
        }

        // Find the entry or create it, then add the stake to it
        let entry = self
            .entries
            .entry(*identity)
            .or_insert_with(StakesEntry::default)
            .add_stake(amount, epoch)?;

        // Because the entry was updated, let's also update all the derived data
        self.stats.latest_epoch = epoch;
        self.stats.average.add_stake(amount, epoch + 1)?;

        Ok(entry)
    }

    /// Tells what is the power of an identity in the network on a certain epoch.
    pub fn query_power(&self, identity: &PublicKeyHash, epoch: Epoch) -> Power {
        self.entries
            .get(identity)
            .map(|entry| entry.power(epoch))
            .unwrap_or_default()
    }

    /// Tells what is the share of the power of an identity in the network on a certain epoch.
    pub fn query_share(&self, identity: &PublicKeyHash, epoch: Epoch) -> f64 {
        let power = self.query_power(identity, epoch);
        let total_power = self.stats.average.power(epoch).max(1);
        let share = (power as f64 / total_power as f64).min(1.0);

        share
    }

    /// Tells how many entries are there in the tracker, paired with some other statistics.
    pub fn stats(&self) -> (usize, &StakingStats) {
        let entries_count = self.entries.len();
        let stats = &self.stats;

        (entries_count, stats)
    }

    /// Remove a certain amount of staked coins from a given identity at a given epoch.
    pub fn remove_stake(
        &mut self,
        identity: &PublicKeyHash,
        amount: Wit,
    ) -> Result<Option<StakesEntry>, StakesTrackerError> {
        // Find the entry or create it, then remove the stake from it
        let entry = self
            .entries
            .entry(*identity)
            .or_insert_with(StakesEntry::default)
            .remove_stake(amount)?
            .clone();

        // If the identity is left without stake, it can be dropped from the tracker
        if entry.coins == Wit::zero() {
            self.entries.remove(identity);
            return Ok(None);
        }

        // Because the entry was updated, let's also update all the derived data
        self.stats.average.remove_stake(amount)?;

        Ok(Some(entry))
    }

    /// Removes and adds an amount of stake at once, i.e. the amount remains the same, but the age
    /// gets reset.
    pub fn use_stake(
        &mut self,
        identity: &PublicKeyHash,
        amount: Wit,
        epoch: Epoch,
    ) -> Result<StakesEntry, StakesTrackerError> {
        // First remove the stake
        self.remove_stake(identity, amount)?;
        // Then add it again at the same epoch
        self.add_stake(identity, amount, epoch).cloned()
    }
}

#[cfg(test)]
mod tests {
    use crate::chain::Environment;

    use super::*;

    #[test]
    fn test_tracker_initialization() {
        let tracker = StakesTracker::default();
        let (count, stats) = tracker.stats();
        assert_eq!(count, 0);
        assert_eq!(stats, &StakingStats::default());
    }

    #[test]
    fn test_add_stake() {
        let mut tracker = StakesTracker::default();
        let alice = PublicKeyHash::from_bech32(
            Environment::Mainnet,
            "wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4",
        )
        .unwrap();
        let bob = PublicKeyHash::from_bech32(
            Environment::Mainnet,
            "wit100000000000000000000000000000000r0v4g2",
        )
        .unwrap();

        // Let's check default power and share
        assert_eq!(tracker.query_power(&alice, 0), 0);
        assert_eq!(tracker.query_share(&alice, 0), 0.0);
        assert_eq!(tracker.query_power(&alice, 1_000), 0);
        assert_eq!(tracker.query_share(&alice, 1_000), 0.0);

        // Let's make Alice stake 100 Wit at epoch 100
        let updated = tracker.add_stake(&alice, Wit::from_wits(100), 100).unwrap();
        assert_eq!(
            updated,
            &StakesEntry {
                coins: Wit::from_wits(100),
                epoch: 100,
                exiting_coins: vec![],
            }
        );
        let (count, stats) = tracker.stats();
        assert_eq!(count, 1);
        assert_eq!(
            stats,
            &StakingStats {
                average: StakesEntry {
                    coins: Wit::from_wits(100),
                    epoch: 101,
                    exiting_coins: vec![],
                },
                latest_epoch: 100,
            }
        );
        assert_eq!(tracker.query_power(&alice, 99), 0);
        assert_eq!(tracker.query_share(&alice, 99), 0.0);
        assert_eq!(tracker.query_power(&alice, 100), 0);
        assert_eq!(tracker.query_share(&alice, 100), 0.0);
        assert_eq!(tracker.query_power(&alice, 101), 100);
        assert_eq!(tracker.query_share(&alice, 101), 1.0);
        assert_eq!(tracker.query_power(&alice, 200), 10_000);
        assert_eq!(tracker.query_share(&alice, 200), 1.0);

        // Let's make Alice stake 50 Wits at epoch 150 this time
        let updated = tracker.add_stake(&alice, Wit::from_wits(50), 300).unwrap();
        assert_eq!(
            updated,
            &StakesEntry {
                coins: Wit::from_wits(150),
                epoch: 166,
                exiting_coins: vec![],
            }
        );
        let (count, stats) = tracker.stats();
        assert_eq!(count, 1);
        assert_eq!(
            stats,
            &StakingStats {
                average: StakesEntry {
                    coins: Wit::from_wits(150),
                    epoch: 167,
                    exiting_coins: vec![],
                },
                latest_epoch: 300,
            }
        );
        assert_eq!(tracker.query_power(&alice, 299), 19_950);
        assert_eq!(tracker.query_share(&alice, 299), 1.0);
        assert_eq!(tracker.query_power(&alice, 300), 20_100);
        assert_eq!(tracker.query_share(&alice, 300), 1.0);
        assert_eq!(tracker.query_power(&alice, 301), 20_250);
        assert_eq!(tracker.query_share(&alice, 301), 1.0);
        assert_eq!(tracker.query_power(&alice, 400), 35_100);
        assert_eq!(tracker.query_share(&alice, 400), 1.0);

        // Now let's make Bob stake 50 Wits at epoch 150 this time
        let updated = tracker.add_stake(&bob, Wit::from_wits(10), 1_000).unwrap();
        assert_eq!(
            updated,
            &StakesEntry {
                coins: Wit::from_wits(10),
                epoch: 1_000,
                exiting_coins: vec![],
            }
        );
        let (count, stats) = tracker.stats();
        assert_eq!(count, 2);
        assert_eq!(
            stats,
            &StakingStats {
                average: StakesEntry {
                    coins: Wit::from_wits(160),
                    epoch: 219,
                    exiting_coins: vec![],
                },
                latest_epoch: 1_000,
            }
        );
        // Before Bob stakes, Alice has all the power and share
        assert_eq!(tracker.query_power(&bob, 999), 0);
        assert_eq!(tracker.query_share(&bob, 999), 0.0);
        assert_eq!(tracker.query_share(&alice, 999), 1.0);
        assert_eq!(
            tracker.query_share(&alice, 999) + tracker.query_share(&bob, 999),
            1.0
        );
        // New stakes don't change power or share in the same epoch
        assert_eq!(tracker.query_power(&bob, 1_000), 0);
        assert_eq!(tracker.query_share(&bob, 1_000), 0.0);
        assert_eq!(tracker.query_share(&alice, 1_000), 1.0);
        assert_eq!(
            tracker.query_share(&alice, 1_000) + tracker.query_share(&bob, 1_000),
            1.0
        );
        // Shortly as Bob's stake gains power, Alice loses a roughly equivalent share
        assert_eq!(tracker.query_power(&bob, 1_100), 1_000);
        assert_eq!(tracker.query_share(&bob, 1_100), 0.007094211123723042);
        assert_eq!(tracker.query_share(&alice, 1_100), 0.9938989784335982);
        assert_eq!(
            tracker.query_share(&alice, 1_100) + tracker.query_share(&bob, 1_100),
            1.0009931895573212
        );
        // After enough time, both's shares should become proportional to their stake, and add up to 1.0 again
        assert_eq!(tracker.query_power(&bob, 1_000_000), 537600);
        assert_eq!(tracker.query_share(&bob, 1_000_000), 0.0625);
        assert_eq!(tracker.query_share(&alice, 1_000_000), 0.9375);
        assert_eq!(
            tracker.query_share(&alice, 1_000_000) + tracker.query_share(&bob, 1_000_000),
            1.0
        );
    }

    #[test]
    fn test_minimum_stake() {
        let mut tracker = StakesTracker::default();
        let alice = PublicKeyHash::from_bech32(
            Environment::Mainnet,
            "wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4",
        )
        .unwrap();
        let error = tracker
            .add_stake(
                &alice,
                Wit::from_wits(MINIMUM_STAKEABLE_AMOUNT_WITS - 1),
                100,
            )
            .unwrap_err();

        assert_eq!(
            error,
            StakesTrackerError::AmountIsBelowMinimum {
                amount: Wit::from_wits(MINIMUM_STAKEABLE_AMOUNT_WITS - 1),
                minimum: Wit::from_wits(MINIMUM_STAKEABLE_AMOUNT_WITS)
            }
        );
    }

    #[test]
    fn test_maximum_coin_age() {
        let mut tracker = StakesTracker::default();
        let alice = PublicKeyHash::from_bech32(
            Environment::Mainnet,
            "wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4",
        )
        .unwrap();
        tracker
            .add_stake(&alice, Wit::from_wits(MINIMUM_STAKEABLE_AMOUNT_WITS), 0)
            .unwrap();
        assert_eq!(tracker.query_power(&alice, 0), 0);
        assert_eq!(
            tracker.query_power(&alice, 1),
            MINIMUM_STAKEABLE_AMOUNT_WITS
        );
        assert_eq!(
            tracker.query_power(&alice, MAXIMUM_COIN_AGE_EPOCHS as Epoch - 1),
            MINIMUM_STAKEABLE_AMOUNT_WITS * (MAXIMUM_COIN_AGE_EPOCHS - 1)
        );
        assert_eq!(
            tracker.query_power(&alice, MAXIMUM_COIN_AGE_EPOCHS as Epoch),
            MINIMUM_STAKEABLE_AMOUNT_WITS * MAXIMUM_COIN_AGE_EPOCHS
        );
        assert_eq!(
            tracker.query_power(&alice, MAXIMUM_COIN_AGE_EPOCHS as Epoch + 1),
            MINIMUM_STAKEABLE_AMOUNT_WITS * MAXIMUM_COIN_AGE_EPOCHS
        );
    }

    #[test]
    fn test_remove_stake() {
        let mut tracker = StakesTracker::default();
        let alice = PublicKeyHash::from_bech32(
            Environment::Mainnet,
            "wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4",
        )
        .unwrap();
        let updated = tracker.add_stake(&alice, Wit::from_wits(100), 100).unwrap();
        assert_eq!(
            updated,
            &StakesEntry {
                coins: Wit::from_wits(100),
                epoch: 100,
                exiting_coins: vec![],
            }
        );
        // Removing stake should reduce the amount, but keep the age the same
        let updated = tracker.remove_stake(&alice, Wit::from_wits(50)).unwrap();
        assert_eq!(
            updated,
            Some(StakesEntry {
                coins: Wit::from_wits(50),
                epoch: 100,
                exiting_coins: vec![],
            })
        );
    }

    #[test]
    fn test_use_stake() {
        let mut tracker = StakesTracker::default();
        let alice = PublicKeyHash::from_bech32(
            Environment::Mainnet,
            "wit1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqwrt3a4",
        )
        .unwrap();
        let updated = tracker.add_stake(&alice, Wit::from_wits(100), 0).unwrap();
        assert_eq!(
            updated,
            &StakesEntry {
                coins: Wit::from_wits(100),
                epoch: 0,
                exiting_coins: vec![],
            }
        );
        // After using all the stake, the amount should stay the same, but the epoch should be reset.
        let updated = tracker.use_stake(&alice, Wit::from_wits(100), 100).unwrap();
        assert_eq!(
            updated,
            StakesEntry {
                coins: Wit::from_wits(100),
                epoch: 100,
                exiting_coins: vec![],
            }
        );
        // But if we use half the stake, again the amount should stay the same, and the epoch should
        // be updated to a point in the middle.
        let updated = tracker.use_stake(&alice, Wit::from_wits(50), 200).unwrap();
        assert_eq!(
            updated,
            StakesEntry {
                coins: Wit::from_wits(100),
                epoch: 150,
                exiting_coins: vec![],
            }
        );
    }
}
