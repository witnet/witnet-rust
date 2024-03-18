use std::{
    collections::{btree_map::Entry, BTreeMap},
    fmt::Debug,
    ops::{Add, Div, Mul, Sub},
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{chain::PublicKeyHash, get_environment, transaction::StakeTransaction, wit::Wit};

use super::prelude::*;

/// The main data structure that provides the "stakes tracker" functionality.
///
/// This structure holds indexes of stake entries. Because the entries themselves are reference
/// counted and write-locked, we can have as many indexes here as we need at a negligible cost.
#[derive(Clone, Debug, Deserialize, Default, PartialEq, Serialize)]
pub struct Stakes<Address, Coins, Epoch, Power>
where
    Address: Default + Ord,
    Coins: Ord,
    Epoch: Default,
{
    /// A listing of all the stakers, indexed by their address.
    by_key: BTreeMap<StakeKey<Address>, SyncStake<Address, Coins, Epoch, Power>>,
    /// A listing of all the stakers, indexed by their coins and address.
    ///
    /// Because this uses a compound key to prevent duplicates, if we want to know which addresses
    /// have staked a particular amount, we just need to run a range lookup on the tree.
    by_coins: BTreeMap<CoinsAndAddresses<Coins, Address>, SyncStake<Address, Coins, Epoch, Power>>,
    /// The amount of coins that can be staked or can be left staked after unstaking.
    /// TODO: reconsider whether this should be here, taking into account that it hinders the possibility of adjusting
    ///  the minimum through TAPI or whatever. Maybe what we can do is set a skip directive for the Serialize macro so
    ///  it never gets persisted and rather always read from constants, or hide the field and the related method
    ///  behind a #[test] thing.
    minimum_stakeable: Option<Coins>,
}

impl<Address, Coins, Epoch, Power> Stakes<Address, Coins, Epoch, Power>
where
    Address: Default,
    Coins: Copy
        + Default
        + Ord
        + From<u64>
        + Into<u64>
        + num_traits::Zero
        + Add<Output = Coins>
        + Sub<Output = Coins>
        + Mul
        + Mul<Epoch, Output = Power>,
    Address: Clone + Ord + 'static,
    Epoch: Copy + Default + num_traits::Saturating + Sub<Output = Epoch> + From<u32>,
    Power: Copy + Default + Ord + Add<Output = Power> + Div<Output = Power>,
    u64: From<Coins> + From<Power>,
{
    /// Register a certain amount of additional stake for a certain address and epoch.
    pub fn add_stake<ISK>(
        &mut self,
        key: ISK,
        coins: Coins,
        epoch: Epoch,
    ) -> StakingResult<Stake<Address, Coins, Epoch, Power>, Address, Coins, Epoch>
    where
        ISK: Into<StakeKey<Address>>,
    {
        let key = key.into();

        // Find or create a matching stake entry
        let stake = self.by_key.entry(key.clone()).or_default();

        // Actually increase the number of coins
        stake
            .value
            .write()?
            .add_stake(coins, epoch, self.minimum_stakeable)?;

        // Update the position of the staker in the `by_coins` index
        // If this staker was not indexed by coins, this will index it now
        let key = CoinsAndAddresses {
            coins,
            addresses: key,
        };
        self.by_coins.remove(&key);
        self.by_coins.insert(key, stake.clone());

        Ok(stake.value.read()?.clone())
    }

    /// Obtain a list of stakers, conveniently ordered by one of several strategies.
    ///
    /// ## Strategies
    ///
    /// - `All`: retrieve all addresses, ordered by decreasing power.
    /// - `StepBy`: retrieve every Nth address, ordered by decreasing power.
    /// - `Take`: retrieve the most powerful N addresses, ordered by decreasing power.
    /// - `Evenly`: retrieve a total of N addresses, evenly distributed from the index, ordered by
    ///   decreasing power.
    pub fn census(
        &self,
        capability: Capability,
        epoch: Epoch,
        strategy: CensusStrategy,
    ) -> Box<dyn Iterator<Item = StakeKey<Address>> + '_> {
        let iterator = self.rank(capability, epoch).map(|(address, _)| address);

        match strategy {
            CensusStrategy::All => Box::new(iterator),
            CensusStrategy::StepBy(step) => Box::new(iterator.step_by(step)),
            CensusStrategy::Take(head) => Box::new(iterator.take(head)),
            CensusStrategy::Evenly(count) => {
                let collected = iterator.collect::<Vec<_>>();
                let step = collected.len() / count;

                Box::new(collected.into_iter().step_by(step).take(count))
            }
        }
    }

    /// Tells what is the power of an identity in the network on a certain epoch.
    pub fn query_power<ISK>(
        &self,
        key: ISK,
        capability: Capability,
        epoch: Epoch,
    ) -> StakingResult<Power, Address, Coins, Epoch>
    where
        ISK: Into<StakeKey<Address>>,
    {
        let key = key.into();

        Ok(self
            .by_key
            .get(&key)
            .ok_or(StakesError::EntryNotFound { key })?
            .value
            .read()?
            .power(capability, epoch))
    }

    /// For a given capability, obtain the full list of stakers ordered by their power in that
    /// capability.
    pub fn rank(
        &self,
        capability: Capability,
        current_epoch: Epoch,
    ) -> impl Iterator<Item = (StakeKey<Address>, Power)> + '_ {
        self.by_coins
            .iter()
            .flat_map(move |(CoinsAndAddresses { addresses, .. }, stake)| {
                stake
                    .value
                    .read()
                    .map(move |stake| (addresses.clone(), stake.power(capability, current_epoch)))
            })
            .sorted_by_key(|(_, power)| *power)
            .rev()
    }

    /// Remove a certain amount of staked coins from a given identity at a given epoch.
    pub fn remove_stake<ISK>(
        &mut self,
        key: ISK,
        coins: Coins,
    ) -> StakingResult<Coins, Address, Coins, Epoch>
    where
        ISK: Into<StakeKey<Address>>,
    {
        let key = key.into();

        if let Entry::Occupied(mut by_address_entry) = self.by_key.entry(key.clone()) {
            let (initial_coins, final_coins) = {
                let mut stake = by_address_entry.get_mut().value.write()?;

                // Check the former amount of stake
                let initial_coins = stake.coins;

                // Reduce the amount of stake
                let final_coins = stake.remove_stake(coins, self.minimum_stakeable)?;

                (initial_coins, final_coins)
            };

            // No need to keep the entry if the stake has gone to zero
            if final_coins.is_zero() {
                by_address_entry.remove();
                self.by_coins.remove(&CoinsAndAddresses {
                    coins: initial_coins,
                    addresses: key,
                });
            }

            Ok(final_coins)
        } else {
            Err(StakesError::EntryNotFound { key })
        }
    }

    /// Set the epoch for a certain address and capability. Most normally, the epoch is the current
    /// epoch.
    pub fn reset_age<ISK>(
        &mut self,
        key: ISK,
        capability: Capability,
        current_epoch: Epoch,
    ) -> StakingResult<(), Address, Coins, Epoch>
    where
        ISK: Into<StakeKey<Address>>,
    {
        let key = key.into();

        let mut stake = self
            .by_key
            .get_mut(&key)
            .ok_or(StakesError::EntryNotFound { key })?
            .value
            .write()?;
        stake.epochs.update(capability, current_epoch);

        Ok(())
    }

    /// Creates an instance of `Stakes` with a custom minimum stakeable amount.
    pub fn with_minimum(minimum: Coins) -> Self {
        Stakes {
            minimum_stakeable: Some(minimum),
            ..Default::default()
        }
    }
}

/// Adds stake, based on the data from a stake transaction.
///
/// This function was made static instead of adding it to `impl Stakes` because it is not generic over `Address` and
/// `Coins`.
pub fn process_stake_transaction<Epoch, Power>(
    stakes: &mut Stakes<PublicKeyHash, Wit, Epoch, Power>,
    transaction: &StakeTransaction,
    epoch: Epoch,
) -> StakingResult<(), PublicKeyHash, Wit, Epoch>
where
    Epoch: Copy + Default + Sub<Output = Epoch> + num_traits::Saturating + From<u32> + Debug,
    Power: Add<Output = Power> + Copy + Default + Div<Output = Power> + Ord + Debug,
    Wit: Mul<Epoch, Output = Power>,
    u64: From<Wit> + From<Power>,
{
    // This line would check that the authorization message is valid for the provided validator and withdrawer
    // address. But it is commented out here because stake transactions should be validated upfront (when
    // considering block candidates). The line is reproduced here for later reference when implementing those
    // validations. Once those are in place, we're ok to remove this comment.
    //transaction.body.authorization_is_valid().map_err(|_| StakesError::InvalidAuthentication)?;

    let key = transaction.body.output.key.clone();
    let coins = Wit::from_nanowits(transaction.body.output.value);

    let environment = get_environment();
    log::debug!(
        "{} added {} Wit more stake on validator {}",
        key.withdrawer.bech32(environment),
        coins.wits_and_nanowits().0,
        key.validator.bech32(environment)
    );

    stakes.add_stake(key, coins, epoch)?;

    log::debug!("Current state of the stakes tracker: {:#?}", stakes);

    Ok(())
}

/// Adds stakes, based on the data from multiple stake transactions.
///
/// This function was made static instead of adding it to `impl Stakes` because it is not generic over `Address` and
/// `Coins`.
pub fn process_stake_transactions<'a, Epoch, Power>(
    stakes: &mut Stakes<PublicKeyHash, Wit, Epoch, Power>,
    transactions: impl Iterator<Item = &'a StakeTransaction>,
    epoch: Epoch,
) -> Result<(), StakesError<PublicKeyHash, Wit, Epoch>>
where
    Epoch: Copy + Default + Sub<Output = Epoch> + num_traits::Saturating + From<u32> + Debug,
    Power: Add<Output = Power> + Copy + Default + Div<Output = Power> + Ord + Debug,
    Wit: Mul<Epoch, Output = Power>,
    u64: From<Wit> + From<Power>,
{
    for transaction in transactions {
        process_stake_transaction(stakes, transaction, epoch)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stakes_initialization() {
        let stakes = Stakes::<String, u64, u64, u64>::default();
        let ranking = stakes.rank(Capability::Mining, 0).collect::<Vec<_>>();
        assert_eq!(ranking, Vec::default());
    }

    #[test]
    fn test_add_stake() {
        let mut stakes = Stakes::<String, u64, u64, u64>::with_minimum(5);
        let alice = "Alice";
        let bob = "Bob";
        let charlie = "Charlie";
        let david = "David";

        let alice_charlie = (alice, charlie);
        let bob_david = (bob, david);

        // Let's check default power
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 0),
            Err(StakesError::EntryNotFound {
                key: StakeKey {
                    validator: alice.into(),
                    withdrawer: charlie.into()
                },
            })
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 1_000),
            Err(StakesError::EntryNotFound {
                key: StakeKey {
                    validator: alice.into(),
                    withdrawer: charlie.into()
                },
            })
        );

        // Let's make Alice stake 100 Wit at epoch 100
        assert_eq!(
            stakes.add_stake(alice_charlie, 100, 100).unwrap(),
            Stake::from_parts(
                100,
                CapabilityMap {
                    mining: 100,
                    witnessing: 100
                }
            )
        );

        // Let's see how Alice's stake accrues power over time
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 99),
            Ok(0)
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 100),
            Ok(0)
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 101),
            Ok(100)
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 200),
            Ok(10_000)
        );

        // Let's make Alice stake 50 Wits at epoch 150 this time
        assert_eq!(
            stakes.add_stake(alice_charlie, 50, 300).unwrap(),
            Stake::from_parts(
                150,
                CapabilityMap {
                    mining: 166,
                    witnessing: 166
                }
            )
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 299),
            Ok(19_950)
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 300),
            Ok(20_100)
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 301),
            Ok(20_250)
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 400),
            Ok(35_100)
        );

        // Now let's make Bob stake 500 Wits at epoch 1000 this time
        assert_eq!(
            stakes.add_stake(bob_david, 500, 1_000).unwrap(),
            Stake::from_parts(
                500,
                CapabilityMap {
                    mining: 1_000,
                    witnessing: 1_000
                }
            )
        );

        // Before Bob stakes, Alice has all the power
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 999),
            Ok(124950)
        );
        assert_eq!(
            stakes.query_power(bob_david, Capability::Mining, 999),
            Ok(0)
        );

        // New stakes don't change power in the same epoch
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 1_000),
            Ok(125100)
        );
        assert_eq!(
            stakes.query_power(bob_david, Capability::Mining, 1_000),
            Ok(0)
        );

        // Shortly after, Bob's stake starts to gain power
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 1_001),
            Ok(125250)
        );
        assert_eq!(
            stakes.query_power(bob_david, Capability::Mining, 1_001),
            Ok(500)
        );

        // After enough time, Bob overpowers Alice
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 2_000),
            Ok(275_100)
        );
        assert_eq!(
            stakes.query_power(bob_david, Capability::Mining, 2_000),
            Ok(500_000)
        );
    }

    #[test]
    fn test_coin_age_resets() {
        // First, lets create a setup with a few stakers
        let mut stakes = Stakes::<String, u64, u64, u64>::with_minimum(5);
        let alice = "Alice";
        let bob = "Bob";
        let charlie = "Charlie";
        let david = "David";
        let erin = "Erin";

        let alice_charlie = (alice, charlie);
        let bob_david = (bob, david);
        let charlie_erin = (charlie, erin);

        stakes.add_stake(alice_charlie, 10, 0).unwrap();
        stakes.add_stake(bob_david, 20, 20).unwrap();
        stakes.add_stake(charlie_erin, 30, 30).unwrap();

        // Let's really start our test at epoch 100
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 100),
            Ok(1_000)
        );
        assert_eq!(
            stakes.query_power(bob_david, Capability::Mining, 100),
            Ok(1_600)
        );
        assert_eq!(
            stakes.query_power(charlie_erin, Capability::Mining, 100),
            Ok(2_100)
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Witnessing, 100),
            Ok(1_000)
        );
        assert_eq!(
            stakes.query_power(bob_david, Capability::Witnessing, 100),
            Ok(1_600)
        );
        assert_eq!(
            stakes.query_power(charlie_erin, Capability::Witnessing, 100),
            Ok(2_100)
        );
        assert_eq!(
            stakes.rank(Capability::Mining, 100).collect::<Vec<_>>(),
            [
                (charlie_erin.into(), 2100),
                (bob_david.into(), 1600),
                (alice_charlie.into(), 1000)
            ]
        );
        assert_eq!(
            stakes.rank(Capability::Witnessing, 100).collect::<Vec<_>>(),
            [
                (charlie_erin.into(), 2100),
                (bob_david.into(), 1600),
                (alice_charlie.into(), 1000)
            ]
        );

        // Now let's slash Charlie's mining coin age right after
        stakes
            .reset_age(charlie_erin, Capability::Mining, 101)
            .unwrap();
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 101),
            Ok(1_010)
        );
        assert_eq!(
            stakes.query_power(bob_david, Capability::Mining, 101),
            Ok(1_620)
        );
        assert_eq!(
            stakes.query_power(charlie_erin, Capability::Mining, 101),
            Ok(0)
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Witnessing, 101),
            Ok(1_010)
        );
        assert_eq!(
            stakes.query_power(bob_david, Capability::Witnessing, 101),
            Ok(1_620)
        );
        assert_eq!(
            stakes.query_power(charlie_erin, Capability::Witnessing, 101),
            Ok(2_130)
        );
        assert_eq!(
            stakes.rank(Capability::Mining, 101).collect::<Vec<_>>(),
            [
                (bob_david.into(), 1_620),
                (alice_charlie.into(), 1_010),
                (charlie_erin.into(), 0)
            ]
        );
        assert_eq!(
            stakes.rank(Capability::Witnessing, 101).collect::<Vec<_>>(),
            [
                (charlie_erin.into(), 2_130),
                (bob_david.into(), 1_620),
                (alice_charlie.into(), 1_010)
            ]
        );

        // Don't panic, Charlie! After enough time, you can take over again ;)
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Mining, 300),
            Ok(3_000)
        );
        assert_eq!(
            stakes.query_power(bob_david, Capability::Mining, 300),
            Ok(5_600)
        );
        assert_eq!(
            stakes.query_power(charlie_erin, Capability::Mining, 300),
            Ok(5_970)
        );
        assert_eq!(
            stakes.query_power(alice_charlie, Capability::Witnessing, 300),
            Ok(3_000)
        );
        assert_eq!(
            stakes.query_power(bob_david, Capability::Witnessing, 300),
            Ok(5_600)
        );
        assert_eq!(
            stakes.query_power(charlie_erin, Capability::Witnessing, 300),
            Ok(8_100)
        );
        assert_eq!(
            stakes.rank(Capability::Mining, 300).collect::<Vec<_>>(),
            [
                (charlie_erin.into(), 5_970),
                (bob_david.into(), 5_600),
                (alice_charlie.into(), 3_000)
            ]
        );
        assert_eq!(
            stakes.rank(Capability::Witnessing, 300).collect::<Vec<_>>(),
            [
                (charlie_erin.into(), 8_100),
                (bob_david.into(), 5_600),
                (alice_charlie.into(), 3_000)
            ]
        );
    }
}