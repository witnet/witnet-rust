use std::collections::btree_map::Entry;
use std::collections::BTreeMap;

use itertools::Itertools;

use super::prelude::*;

/// The main data structure that provides the "stakes tracker" functionality.
///
/// This structure holds indexes of stake entries. Because the entries themselves are reference
/// counted and write-locked, we can have as many indexes here as we need at a negligible cost.
#[derive(Default)]
pub struct Stakes<Address, Coins, Epoch, Power>
where
    Address: Default,
    Epoch: Default,
{
    /// A listing of all the stakers, indexed by their address.
    by_address: BTreeMap<Address, SyncStake<Address, Coins, Epoch, Power>>,
    /// A listing of all the stakers, indexed by their coins and address.
    ///
    /// Because this uses a compound key to prevent duplicates, if we want to know which addresses
    /// have staked a particular amount, we just need to run a range lookup on the tree.
    by_coins: BTreeMap<CoinsAndAddress<Coins, Address>, SyncStake<Address, Coins, Epoch, Power>>,
    /// The amount of coins that can be staked or can be left staked after unstaking.
    minimum_stakeable: Option<Coins>,
}

impl<Address, Coins, Epoch, Power> Stakes<Address, Coins, Epoch, Power>
where
    Address: Default,
    Coins: Copy
        + Default
        + Ord
        + From<u64>
        + num_traits::Zero
        + std::ops::Add<Output = Coins>
        + std::ops::Sub<Output = Coins>
        + std::ops::Mul
        + std::ops::Mul<Epoch, Output = Power>,
    Address: Clone + Ord + 'static,
    Epoch: Copy + Default + num_traits::Saturating + std::ops::Sub<Output = Epoch>,
    Power: Copy
        + Default
        + Ord
        + std::ops::Add<Output = Power>
        + std::ops::Div<Output = Power>
        + std::ops::Div<Coins, Output = Epoch>
        + 'static,
{
    /// Register a certain amount of additional stake for a certain address and epoch.
    pub fn add_stake<IA>(
        &mut self,
        address: IA,
        coins: Coins,
        epoch: Epoch,
    ) -> Result<Stake<Address, Coins, Epoch, Power>, Address, Coins, Epoch>
    where
        IA: Into<Address>,
    {
        let address = address.into();
        let stake_arc = self.by_address.entry(address.clone()).or_default();

        // Actually increase the number of coins
        stake_arc
            .write()?
            .add_stake(coins, epoch, self.minimum_stakeable)?;

        // Update the position of the staker in the `by_coins` index
        // If this staker was not indexed by coins, this will index it now
        let key = CoinsAndAddress {
            coins,
            address: address.clone(),
        };
        self.by_coins.remove(&key);
        self.by_coins.insert(key, stake_arc.clone());

        Ok(stake_arc.read()?.clone())
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
    ) -> Box<dyn Iterator<Item = Address>> {
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
    pub fn query_power(
        &self,
        address: &Address,
        capability: Capability,
        epoch: Epoch,
    ) -> Result<Power, Address, Coins, Epoch> {
        Ok(self
            .by_address
            .get(address)
            .ok_or(StakesError::IdentityNotFound {
                identity: address.clone(),
            })?
            .read()?
            .power(capability, epoch))
    }

    /// For a given capability, obtain the full list of stakers ordered by their power in that
    /// capability.
    pub fn rank(
        &self,
        capability: Capability,
        current_epoch: Epoch,
    ) -> impl Iterator<Item = (Address, Power)> + 'static {
        self.by_coins
            .iter()
            .flat_map(move |(CoinsAndAddress { address, .. }, stake)| {
                stake
                    .read()
                    .map(move |stake| (address.clone(), stake.power(capability, current_epoch)))
            })
            .sorted_by_key(|(_, power)| *power)
            .rev()
    }

    /// Remove a certain amount of staked coins from a given identity at a given epoch.
    pub fn remove_stake<IA>(
        &mut self,
        address: IA,
        coins: Coins,
    ) -> Result<Coins, Address, Coins, Epoch>
    where
        IA: Into<Address>,
    {
        let address = address.into();
        if let Entry::Occupied(mut by_address_entry) = self.by_address.entry(address.clone()) {
            let (initial_coins, final_coins) = {
                let mut stake = by_address_entry.get_mut().write()?;

                // Check the former amount of stake
                let initial_coins = stake.coins;

                // Reduce the amount of stake
                let final_coins = stake.remove_stake(coins, self.minimum_stakeable)?;

                (initial_coins, final_coins)
            };

            // No need to keep the entry if the stake has gone to zero
            if final_coins.is_zero() {
                by_address_entry.remove();
                self.by_coins.remove(&CoinsAndAddress {
                    coins: initial_coins,
                    address,
                });
            }

            Ok(final_coins)
        } else {
            Err(StakesError::IdentityNotFound { identity: address })
        }
    }

    /// Set the epoch for a certain address and capability. Most normally, the epoch is the current
    /// epoch.
    pub fn reset_age<IA>(
        &mut self,
        address: IA,
        capability: Capability,
        current_epoch: Epoch,
    ) -> Result<(), Address, Coins, Epoch>
    where
        IA: Into<Address>,
    {
        let address = address.into();
        let mut stake = self
            .by_address
            .get_mut(&address)
            .ok_or(StakesError::IdentityNotFound { identity: address })?
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
        let alice = "Alice".into();
        let bob = "Bob".into();

        // Let's check default power
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 0),
            Err(StakesError::IdentityNotFound {
                identity: alice.clone()
            })
        );
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 1_000),
            Err(StakesError::IdentityNotFound {
                identity: alice.clone()
            })
        );

        // Let's make Alice stake 100 Wit at epoch 100
        assert_eq!(
            stakes.add_stake(&alice, 100, 100).unwrap(),
            Stake::from_parts(
                100,
                CapabilityMap {
                    mining: 100,
                    witnessing: 100
                }
            )
        );

        // Let's see how Alice's stake accrues power over time
        assert_eq!(stakes.query_power(&alice, Capability::Mining, 99), Ok(0));
        assert_eq!(stakes.query_power(&alice, Capability::Mining, 100), Ok(0));
        assert_eq!(stakes.query_power(&alice, Capability::Mining, 101), Ok(100));
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 200),
            Ok(10_000)
        );

        // Let's make Alice stake 50 Wits at epoch 150 this time
        assert_eq!(
            stakes.add_stake(&alice, 50, 300).unwrap(),
            Stake::from_parts(
                150,
                CapabilityMap {
                    mining: 166,
                    witnessing: 166
                }
            )
        );
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 299),
            Ok(19_950)
        );
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 300),
            Ok(20_100)
        );
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 301),
            Ok(20_250)
        );
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 400),
            Ok(35_100)
        );

        // Now let's make Bob stake 500 Wits at epoch 1000 this time
        assert_eq!(
            stakes.add_stake(&bob, 500, 1_000).unwrap(),
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
            stakes.query_power(&alice, Capability::Mining, 999),
            Ok(124950)
        );
        assert_eq!(stakes.query_power(&bob, Capability::Mining, 999), Ok(0));

        // New stakes don't change power in the same epoch
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 1_000),
            Ok(125100)
        );
        assert_eq!(stakes.query_power(&bob, Capability::Mining, 1_000), Ok(0));

        // Shortly after, Bob's stake starts to gain power
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 1_001),
            Ok(125250)
        );
        assert_eq!(stakes.query_power(&bob, Capability::Mining, 1_001), Ok(500));

        // After enough time, Bob overpowers Alice
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 2_000),
            Ok(275_100)
        );
        assert_eq!(
            stakes.query_power(&bob, Capability::Mining, 2_000),
            Ok(500_000)
        );
    }

    #[test]
    fn test_coin_age_resets() {
        // First, lets create a setup with a few stakers
        let mut stakes = Stakes::<String, u64, u64, u64>::with_minimum(5);
        let alice = "Alice".into();
        let bob = "Bob".into();
        let charlie = "Charlie".into();

        stakes.add_stake(&alice, 10, 0).unwrap();
        stakes.add_stake(&bob, 20, 20).unwrap();
        stakes.add_stake(&charlie, 30, 30).unwrap();

        // Let's really start our test at epoch 100
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 100),
            Ok(1_000)
        );
        assert_eq!(stakes.query_power(&bob, Capability::Mining, 100), Ok(1_600));
        assert_eq!(
            stakes.query_power(&charlie, Capability::Mining, 100),
            Ok(2_100)
        );
        assert_eq!(
            stakes.query_power(&alice, Capability::Witnessing, 100),
            Ok(1_000)
        );
        assert_eq!(
            stakes.query_power(&bob, Capability::Witnessing, 100),
            Ok(1_600)
        );
        assert_eq!(
            stakes.query_power(&charlie, Capability::Witnessing, 100),
            Ok(2_100)
        );
        assert_eq!(
            stakes.rank(Capability::Mining, 100).collect::<Vec<_>>(),
            [
                (charlie.clone(), 2100),
                (bob.clone(), 1600),
                (alice.clone(), 1000)
            ]
        );
        assert_eq!(
            stakes.rank(Capability::Witnessing, 100).collect::<Vec<_>>(),
            [
                (charlie.clone(), 2100),
                (bob.clone(), 1600),
                (alice.clone(), 1000)
            ]
        );

        // Now let's slash Charlie's mining coin age right after
        stakes.reset_age(&charlie, Capability::Mining, 101).unwrap();
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 101),
            Ok(1_010)
        );
        assert_eq!(stakes.query_power(&bob, Capability::Mining, 101), Ok(1_620));
        assert_eq!(stakes.query_power(&charlie, Capability::Mining, 101), Ok(0));
        assert_eq!(
            stakes.query_power(&alice, Capability::Witnessing, 101),
            Ok(1_010)
        );
        assert_eq!(
            stakes.query_power(&bob, Capability::Witnessing, 101),
            Ok(1_620)
        );
        assert_eq!(
            stakes.query_power(&charlie, Capability::Witnessing, 101),
            Ok(2_130)
        );
        assert_eq!(
            stakes.rank(Capability::Mining, 101).collect::<Vec<_>>(),
            [
                (bob.clone(), 1_620),
                (alice.clone(), 1_010),
                (charlie.clone(), 0)
            ]
        );
        assert_eq!(
            stakes.rank(Capability::Witnessing, 101).collect::<Vec<_>>(),
            [
                (charlie.clone(), 2_130),
                (bob.clone(), 1_620),
                (alice.clone(), 1_010)
            ]
        );

        // Don't panic, Charlie! After enough time, you can take over again ;)
        assert_eq!(
            stakes.query_power(&alice, Capability::Mining, 300),
            Ok(3_000)
        );
        assert_eq!(stakes.query_power(&bob, Capability::Mining, 300), Ok(5_600));
        assert_eq!(
            stakes.query_power(&charlie, Capability::Mining, 300),
            Ok(5_970)
        );
        assert_eq!(
            stakes.query_power(&alice, Capability::Witnessing, 300),
            Ok(3_000)
        );
        assert_eq!(
            stakes.query_power(&bob, Capability::Witnessing, 300),
            Ok(5_600)
        );
        assert_eq!(
            stakes.query_power(&charlie, Capability::Witnessing, 300),
            Ok(8_100)
        );
        assert_eq!(
            stakes.rank(Capability::Mining, 300).collect::<Vec<_>>(),
            [
                (charlie.clone(), 5_970),
                (bob.clone(), 5_600),
                (alice.clone(), 3_000)
            ]
        );
        assert_eq!(
            stakes.rank(Capability::Witnessing, 300).collect::<Vec<_>>(),
            [
                (charlie.clone(), 8_100),
                (bob.clone(), 5_600),
                (alice.clone(), 3_000)
            ]
        );
    }
}
