use std::{
    collections::{btree_map::Entry, BTreeMap},
    fmt::{Debug, Display},
    iter::Sum,
    ops::{Add, AddAssign, Div, Mul, Rem, Sub},
};

use itertools::Itertools;
use num_traits::Saturating;
use serde::{Deserialize, Serialize};

use crate::{
    chain::{Epoch, PublicKeyHash},
    get_environment,
    transaction::{StakeTransaction, UnstakeTransaction},
    wit::{PrecisionLoss, Wit, WIT_DECIMAL_PLACES},
};

use super::prelude::*;

/// Message for querying stakes
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum QueryStakesKey<Address: Default + Ord> {
    /// Query stakes by validator address
    Validator(Address),
    /// Query stakes by withdrawer address
    Withdrawer(Address),
    /// Query stakes by validator and withdrawer addresses
    Key(StakeKey<Address>),
}

impl<Address> Default for QueryStakesKey<Address>
where
    Address: Default + Ord,
{
    fn default() -> Self {
        QueryStakesKey::Validator(Address::default())
    }
}

impl<Address, T> TryFrom<(Option<T>, Option<T>)> for QueryStakesKey<Address>
where
    Address: Default + Ord,
    T: Into<Address>,
{
    type Error = String;
    fn try_from(val: (Option<T>, Option<T>)) -> Result<Self, Self::Error> {
        match val {
            (Some(validator), Some(withdrawer)) => Ok(QueryStakesKey::Key(StakeKey {
                validator: validator.into(),
                withdrawer: withdrawer.into(),
            })),
            (Some(validator), _) => Ok(QueryStakesKey::Validator(validator.into())),
            (_, Some(withdrawer)) => Ok(QueryStakesKey::Withdrawer(withdrawer.into())),
            _ => Err(String::from(
                "Either a validator address, a withdrawer address or both must be provided.",
            )),
        }
    }
}

/// The main data structure that provides the "stakes tracker" functionality.
///
/// This structure holds indexes of stake entries. Because the entries themselves are reference
/// counted and write-locked, we can have as many indexes here as we need at a negligible cost.
#[allow(clippy::type_complexity)]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stakes<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default + Ord,
    Coins: Clone + Ord,
    Epoch: Clone + Default,
    Nonce: Clone + Default,
    Power: Clone,
{
    /// A listing of all the stake entries, indexed by their stake key.
    pub(crate) by_key:
        BTreeMap<StakeKey<Address>, SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>>,
    /// A listing of all the stake entries, indexed by validator.
    by_validator: BTreeMap<Address, Vec<SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>>>,
    /// A listing of all the stake entries, indexed by withdrawer.
    by_withdrawer:
        BTreeMap<Address, Vec<SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>>>,
    /// A listing of all the stake entries, indexed by their coins and address.
    ///
    /// Because this uses a compound key to prevent duplicates, if we want to know which addresses
    /// have staked a particular amount, we just need to run a range lookup on the tree.
    by_coins: BTreeMap<
        CoinsAndAddresses<Coins, Address>,
        SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>,
    >,
}

impl<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
    Stakes<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Debug + Default + Ord + Send + Serialize + Sync + Display + 'static,
    Coins: Copy
        + Default
        + Ord
        + From<u64>
        + Into<u64>
        + num_traits::Zero
        + Add<Output = Coins>
        + Sub<Output = Coins>
        + Mul
        + Mul<Epoch, Output = Power>
        + Debug
        + Send
        + Sync
        + Display
        + Serialize
        + Sum
        + Div<Output = Coins>
        + Rem<Output = Coins>
        + PrecisionLoss,
    Epoch: Copy
        + Default
        + Saturating
        + Sub<Output = Epoch>
        + From<u32>
        + Debug
        + Display
        + Send
        + Serialize
        + Sync
        + Add<Output = Epoch>
        + Div<Output = Epoch>,
    Nonce: AddAssign
        + Copy
        + Debug
        + Default
        + Display
        + From<u32>
        + Saturating
        + Send
        + Serialize
        + Sync,
    Power: Copy + Default + Ord + Add<Output = Power> + Div<Output = Power> + Serialize + Sum,
    u64: From<Coins> + From<Power>,
{
    /// Register a certain amount of additional stake for a certain address, capability and epoch.
    pub fn add_stake<ISK>(
        &mut self,
        key: ISK,
        coins: Coins,
        epoch: Epoch,
        minimum_stakeable: Coins,
    ) -> StakesResult<Stake<UNIT, Address, Coins, Epoch, Nonce, Power>, Address, Coins, Epoch>
    where
        ISK: Into<StakeKey<Address>>,
    {
        let key = key.into();

        // Find or create a matching stake entry
        let stake_found = self.by_key.contains_key(&key);
        let stake = self
            .by_key
            .entry(key.clone())
            .or_insert(SyncStakeEntry::from(StakeEntry {
                key: key.clone(),
                ..Default::default()
            }));

        if !stake_found {
            stake.key.write()?.validator = key.validator.clone();
            stake.key.write()?.withdrawer = key.withdrawer.clone();
        }

        // Actually increase the number of coins
        stake
            .value
            .write()?
            .add_stake(coins, epoch, minimum_stakeable)?;

        // Update all indexes if needed (only when the stake entry didn't exist before)
        index_coins(&mut self.by_coins, key.clone(), stake.clone());
        if !stake_found {
            index_addresses(
                &mut self.by_validator,
                &mut self.by_withdrawer,
                key,
                stake.clone(),
            );
        }

        Ok(stake.value.read()?.clone())
    }

    /// Quickly count how many stake entries are recorded into this data structure.
    pub fn stakes_count(&self) -> usize {
        self.by_key.len()
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
    ) -> Box<dyn Iterator<Item = Address> + '_> {
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
        validator: ISK,
        capability: Capability,
        epoch: Epoch,
    ) -> StakesResult<Power, Address, Coins, Epoch>
    where
        ISK: Into<Address>,
    {
        let validator = validator.into();

        let validator = self
            .by_validator
            .get(&validator)
            .ok_or(StakesError::ValidatorNotFound { validator })?;

        Ok(validator
            .iter()
            .map(|stake| stake.read_value().power(capability, epoch))
            .collect::<Vec<Power>>()
            .into_iter()
            .sum())
    }

    /// For a given capability, obtain the full list of stakers ordered by their power in that
    /// capability.
    /// TODO: we may memoize the rank by keeping the last one in a non-serializable field in `Self` that keeps a boxed
    ///  iterator, so that this method doesn't have to sort multiple times if we are calling the `rank` method several
    ///  times in the same epoch.
    pub fn rank(
        &self,
        capability: Capability,
        current_epoch: Epoch,
    ) -> impl Iterator<Item = (Address, Power)> + '_ {
        self.by_validator
            .iter()
            .map(move |(address, stakes)| {
                let power = stakes
                    .first()
                    .unwrap()
                    .read_value()
                    .power(capability, current_epoch);

                (address.clone(), power)
            })
            .sorted_by_key(|(_, power)| *power)
            .rev()
    }

    /// Query the current nonce from a stake entry.
    pub fn query_nonce<ISK>(&mut self, key: ISK) -> StakesResult<Nonce, Address, Coins, Epoch>
    where
        ISK: Into<StakeKey<Address>>,
    {
        let key = key.into();

        if let Entry::Occupied(entry) = self.by_key.entry(key.clone()) {
            let stake = entry.get().value.read()?;

            Ok(stake.nonce)
        } else {
            Err(StakesError::EntryNotFound { key })
        }
    }

    /// Remove a certain amount of staked coins from a given identity at a given epoch.
    pub fn remove_stake<ISK>(
        &mut self,
        key: ISK,
        coins: Coins,
        minimum_stakeable: Coins,
    ) -> StakesResult<Coins, Address, Coins, Epoch>
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
                let final_coins = stake.remove_stake(coins, minimum_stakeable)?;

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
        validator: ISK,
        capability: Capability,
        current_epoch: Epoch,
        reset_factor: u32,
    ) -> StakesResult<(), Address, Coins, Epoch>
    where
        ISK: Into<Address>,
    {
        let validator = validator.into();

        let stakes = self
            .by_validator
            .get_mut(&validator)
            .ok_or(StakesError::ValidatorNotFound { validator })?;
        stakes.iter_mut().for_each(|stake| {
            let old_epoch = stake.value.read().unwrap().epochs.get(capability);
            let update_epoch = (current_epoch - old_epoch) / Epoch::from(reset_factor);
            stake
                .value
                .write()
                .unwrap()
                .epochs
                .update(capability, old_epoch + update_epoch);
        });

        Ok(())
    }

    /// Add a reward to the validator's balance
    pub fn add_reward<ISK>(
        &mut self,
        validator: ISK,
        coins: Coins,
        current_epoch: Epoch,
    ) -> StakesResult<(), Address, Coins, Epoch>
    where
        ISK: Into<Address>,
    {
        let validator = validator.into();

        let stakes = self
            .by_validator
            .get_mut(&validator)
            .ok_or(StakesError::ValidatorNotFound { validator })?;

        // TODO: modify this to enable delegated staking with multiple withdrawer addresses on a single validator
        let _ = stakes[0]
            .value
            .write()
            .unwrap()
            .add_stake(coins, current_epoch, 0.into());

        Ok(())
    }

    /// Add a reward to the validator's balance
    pub fn reserve_collateral<ISK>(
        &mut self,
        validator: ISK,
        coins: Coins,
        minimum_stakeable: Coins,
    ) -> StakesResult<(), Address, Coins, Epoch>
    where
        ISK: Into<Address>,
    {
        let validator = validator.into();

        let stakes = self
            .by_validator
            .get_mut(&validator)
            .ok_or(StakesError::ValidatorNotFound { validator })?;

        // TODO: modify this to enable delegated staking with multiple withdrawer addresses on a single validator
        let _ = stakes[0]
            .value
            .write()
            .unwrap()
            .remove_stake(coins, minimum_stakeable);

        Ok(())
    }

    /// Creates an instance of `Stakes` that is initialized with a existing set of stake entries.
    ///
    /// This is specially convenient after loading stakes from storage, as this function rebuilds
    /// all the indexes at once to preserve write locks and reference counts.
    pub fn with_entries(
        entries: BTreeMap<
            StakeKey<Address>,
            SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>,
        >,
    ) -> Self {
        let mut stakes = Stakes {
            by_key: entries,
            ..Default::default()
        };
        stakes.reindex();

        stakes
    }

    /// Rebuild all indexes based on the entries found in `by_key`.
    pub fn reindex(&mut self) {
        self.by_validator.clear();
        self.by_withdrawer.clear();
        self.by_coins.clear();

        for (key, stake) in &self.by_key {
            index_coins(&mut self.by_coins, key.clone(), stake.clone());
            index_addresses(
                &mut self.by_validator,
                &mut self.by_withdrawer,
                key.clone(),
                stake.clone(),
            );
        }
    }

    /// Query stakes based on different keys.
    pub fn query_stakes<TIQSK>(
        &self,
        query: TIQSK,
    ) -> StakeEntryVecResult<UNIT, Address, Coins, Epoch, Nonce, Power>
    where
        TIQSK: TryInto<QueryStakesKey<Address>>,
    {
        match query.try_into() {
            Ok(QueryStakesKey::Key(key)) => self.query_by_key(key).map(|stake| vec![stake]),
            Ok(QueryStakesKey::Validator(validator)) => self.query_by_validator(validator),
            Ok(QueryStakesKey::Withdrawer(withdrawer)) => self.query_by_withdrawer(withdrawer),
            Err(_) => Err(StakesError::EmptyQuery),
        }
    }

    /// Query the total amount of stake based on different keys.
    pub fn query_total_stake<TIQSK>(
        &mut self,
        query: TIQSK,
    ) -> StakesResult<Coins, Address, Coins, Epoch>
    where
        TIQSK: TryInto<QueryStakesKey<Address>>,
    {
        totalize_stakes(
            self.query_stakes(query)?
                .into_iter()
                .map(|entry| entry.value),
        )
    }

    /// Return the total number of validators.
    pub fn validator_count(&self) -> usize {
        self.by_validator.len()
    }

    /// Return the total number staked.
    pub fn total_staked(&self) -> Coins {
        self.by_key
            .values()
            .map(|entry| entry.value.read().unwrap().coins)
            .collect::<Vec<Coins>>()
            .into_iter()
            .sum()
    }

    /// Query stakes to check for an existing validator / withdrawer pair.
    pub fn check_validator_withdrawer<ISK>(
        &self,
        validator: ISK,
        withdrawer: ISK,
    ) -> StakesResult<(), Address, Coins, Epoch>
    where
        ISK: Into<Address>,
    {
        let validator = validator.into();
        let withdrawer = withdrawer.into();

        if !self.by_validator.contains_key(&validator) {
            Ok(())
        } else {
            let stake_key = StakeKey::from((validator.clone(), withdrawer));
            if self.by_key.contains_key(&stake_key) {
                Ok(())
            } else {
                Err(StakesError::DifferentWithdrawer { validator })
            }
        }
    }

    /// Query stakes by stake key.
    #[inline(always)]
    fn query_by_key(
        &self,
        key: StakeKey<Address>,
    ) -> StakeEntryResult<UNIT, Address, Coins, Epoch, Nonce, Power> {
        Ok(self
            .by_key
            .get(&key)
            .ok_or(StakesError::EntryNotFound { key })?
            .read_entry())
    }

    /// Query stakes by validator address.
    #[inline(always)]
    fn query_by_validator(
        &self,
        validator: Address,
    ) -> StakeEntryVecResult<UNIT, Address, Coins, Epoch, Nonce, Power> {
        let validator = self
            .by_validator
            .get(&validator)
            .ok_or(StakesError::ValidatorNotFound { validator })?;

        Ok(validator.iter().map(SyncStakeEntry::read_entry).collect())
    }

    /// Query stakes by withdrawer address.
    #[inline(always)]
    fn query_by_withdrawer(
        &self,
        withdrawer: Address,
    ) -> StakeEntryVecResult<UNIT, Address, Coins, Epoch, Nonce, Power> {
        let withdrawer = self
            .by_withdrawer
            .get(&withdrawer)
            .ok_or(StakesError::WithdrawerNotFound { withdrawer })?;

        Ok(withdrawer.iter().map(SyncStakeEntry::read_entry).collect())
    }
}

/// The default concrete type for tracking stakes in the node software.
pub type StakesTracker = Stakes<WIT_DECIMAL_PLACES, PublicKeyHash, Wit, Epoch, u64, u64>;

/// The default concrete type for testing stakes in unit tests.
pub type StakesTester = Stakes<0, String, u64, u64, u64, u64>;

/// Update the position of the staker in a `by_coins` index.
/// If this stake entry was not indexed by coins, this will add it to the index.
///
/// This function was made static instead of adding it to `impl Stakes` because of limitations
#[allow(clippy::type_complexity)]
pub fn index_coins<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>(
    by_coins: &mut BTreeMap<
        CoinsAndAddresses<Coins, Address>,
        SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>,
    >,
    key: StakeKey<Address>,
    stake: SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>,
) where
    Address: Clone + Default + Ord,
    Coins: Copy + Default + Ord,
    Epoch: Clone + Default,
    Nonce: Clone + Default,
    Power: Clone + Default,
{
    let coins_and_addresses = CoinsAndAddresses {
        coins: stake.value.read().unwrap().coins,
        addresses: key.clone(),
    };

    by_coins.remove(&coins_and_addresses);
    by_coins.insert(coins_and_addresses.clone(), stake.clone());
}

/// Upsert a stake entry into those indexes that allow querying by validator or withdrawer.
#[allow(clippy::type_complexity)]
pub fn index_addresses<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>(
    by_validator: &mut BTreeMap<
        Address,
        Vec<SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>>,
    >,
    by_withdrawer: &mut BTreeMap<
        Address,
        Vec<SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>>,
    >,
    key: StakeKey<Address>,
    stake: SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>,
) where
    Address: Clone + Default + Ord,
    Coins: Clone + Default + Ord,
    Epoch: Clone + Default,
    Nonce: Clone + Default,
    Power: Clone + Default,
{
    let validator_key = key.validator;
    if let Some(validator) = by_validator.get_mut(&validator_key) {
        validator.push(stake.clone());
    } else {
        by_validator.insert(validator_key, vec![stake.clone()]);
    }

    let withdrawer_key = key.withdrawer;
    if let Some(withdrawer) = by_withdrawer.get_mut(&withdrawer_key) {
        withdrawer.push(stake);
    } else {
        by_withdrawer.insert(withdrawer_key, vec![stake]);
    }
}

/// Adds stake, based on the data from a stake transaction.
///
/// This function was made static instead of adding it to `impl Stakes` because it is not generic over `Address` and
/// `Coins`.
#[allow(clippy::type_complexity)]
pub fn process_stake_transaction<const UNIT: u8, Epoch, Nonce, Power>(
    stakes: &mut Stakes<UNIT, PublicKeyHash, Wit, Epoch, Nonce, Power>,
    transaction: &StakeTransaction,
    epoch: Epoch,
    minimum_stakeable: u64,
) -> StakesResult<(), PublicKeyHash, Wit, Epoch>
where
    Epoch: Copy
        + Default
        + Sub<Output = Epoch>
        + Saturating
        + From<u32>
        + Debug
        + Display
        + Send
        + Serialize
        + Sync
        + Add<Output = Epoch>
        + Div<Output = Epoch>,
    Nonce: AddAssign
        + Copy
        + Debug
        + Default
        + Display
        + From<u32>
        + Saturating
        + Send
        + Serialize
        + Sync,
    Power:
        Add<Output = Power> + Copy + Debug + Default + Div<Output = Power> + Ord + Serialize + Sum,
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

    stakes.add_stake(key, coins, epoch, minimum_stakeable.into())?;

    log::debug!("Current state of the stakes tracker: {:#?}", stakes);

    Ok(())
}

/// Removes stake, based on the data from a unstake transaction.
///
/// This function was made static instead of adding it to `impl Stakes` because it is not generic over `Address` and
/// `Coins`.
pub fn process_unstake_transaction<const UNIT: u8, Epoch, Nonce, Power>(
    stakes: &mut Stakes<UNIT, PublicKeyHash, Wit, Epoch, Nonce, Power>,
    transaction: &UnstakeTransaction,
    minimum_stakeable: u64,
) -> StakesResult<(), PublicKeyHash, Wit, Epoch>
where
    Epoch: Copy
        + Default
        + Sub<Output = Epoch>
        + Saturating
        + From<u32>
        + Debug
        + Display
        + Send
        + Serialize
        + Sync
        + Add<Output = Epoch>
        + Div<Output = Epoch>,
    Nonce: AddAssign
        + Copy
        + Debug
        + Default
        + Display
        + From<u32>
        + Saturating
        + Send
        + Serialize
        + Sync,
    Power:
        Add<Output = Power> + Copy + Debug + Default + Div<Output = Power> + Ord + Serialize + Sum,
    Wit: Mul<Epoch, Output = Power>,
    u64: From<Wit> + From<Power>,
{
    let key: StakeKey<PublicKeyHash> = StakeKey {
        validator: transaction.body.operator,
        withdrawer: transaction.body.withdrawal.pkh,
    };

    let coins = Wit::from_nanowits(transaction.body.withdrawal.value + transaction.body.fee);

    let environment = get_environment();
    log::debug!(
        "{} removed {} Wit stake",
        key.validator.bech32(environment),
        coins.wits_and_nanowits().0,
    );

    stakes.remove_stake(key, coins, minimum_stakeable.into())?;

    log::debug!("Current state of the stakes tracker: {:#?}", stakes);

    Ok(())
}

/// Adds stakes, based on the data from multiple stake transactions.
///
/// This function was made static instead of adding it to `impl Stakes` because it is not generic over `Address` and
/// `Coins`.
pub fn process_stake_transactions<'a, const UNIT: u8, Epoch, Nonce, Power>(
    stakes: &mut Stakes<UNIT, PublicKeyHash, Wit, Epoch, Nonce, Power>,
    transactions: impl Iterator<Item = &'a StakeTransaction>,
    epoch: Epoch,
    minimum_stakeable: u64,
) -> Result<(), StakesError<PublicKeyHash, Wit, Epoch>>
where
    Epoch: Copy
        + Default
        + Sub<Output = Epoch>
        + Saturating
        + From<u32>
        + Debug
        + Send
        + Serialize
        + Sync
        + Display
        + Add<Output = Epoch>
        + Div<Output = Epoch>,
    Nonce: AddAssign
        + Copy
        + Debug
        + Default
        + Display
        + From<u32>
        + Saturating
        + Send
        + Serialize
        + Sync,
    Power:
        Add<Output = Power> + Copy + Debug + Default + Div<Output = Power> + Ord + Serialize + Sum,
    Wit: Mul<Epoch, Output = Power>,
    u64: From<Wit> + From<Power>,
{
    for transaction in transactions {
        process_stake_transaction(stakes, transaction, epoch, minimum_stakeable)?;
    }

    Ok(())
}
/// Removes stakes, based on the data from multiple unstake transactions.
///
/// This function was made static instead of adding it to `impl Stakes` because it is not generic over `Address` and
/// `Coins`.
pub fn process_unstake_transactions<'a, const UNIT: u8, Epoch, Nonce, Power>(
    stakes: &mut Stakes<UNIT, PublicKeyHash, Wit, Epoch, Nonce, Power>,
    transactions: impl Iterator<Item = &'a UnstakeTransaction>,
    minimum_stakeable: u64,
) -> Result<(), StakesError<PublicKeyHash, Wit, Epoch>>
where
    Epoch: Copy
        + Default
        + Sub<Output = Epoch>
        + Saturating
        + From<u32>
        + Debug
        + Send
        + Serialize
        + Sync
        + Display
        + Add<Output = Epoch>
        + Div<Output = Epoch>,
    Nonce: AddAssign
        + Copy
        + Debug
        + Default
        + Display
        + From<u32>
        + Saturating
        + Send
        + Serialize
        + Sync,
    Power:
        Add<Output = Power> + Copy + Debug + Default + Div<Output = Power> + Ord + Serialize + Sum,
    Wit: Mul<Epoch, Output = Power>,
    u64: From<Wit> + From<Power>,
{
    for transaction in transactions {
        process_unstake_transaction(stakes, transaction, minimum_stakeable)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_STAKE_NANOWITS: u64 = 1;

    #[test]
    fn test_stakes_initialization() {
        let stakes = StakesTester::default();
        let ranking = stakes.rank(Capability::Mining, 0).collect::<Vec<_>>();
        assert_eq!(ranking, Vec::default());
    }

    #[test]
    fn test_add_stake() {
        let mut stakes = StakesTester::default();
        let alice = "Alice";
        let bob = "Bob";
        let charlie = "Charlie";
        let david = "David";

        let alice_charlie = (alice, charlie);
        let bob_david = (bob, david);

        // Let's check default power
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 0),
            Err(StakesError::ValidatorNotFound {
                validator: alice.into(),
            })
        );
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 1_000),
            Err(StakesError::ValidatorNotFound {
                validator: alice.into(),
            })
        );

        // Let's make Alice stake 100 Wit at epoch 100
        assert_eq!(
            stakes
                .add_stake(alice_charlie, 100, 100, MIN_STAKE_NANOWITS)
                .unwrap(),
            Stake::from_parts(
                100,
                CapabilityMap {
                    mining: 100,
                    witnessing: 100
                },
                1,
            )
        );

        // Let's see how Alice's stake accrues power over time
        assert_eq!(stakes.query_power(alice, Capability::Mining, 99), Ok(0));
        assert_eq!(stakes.query_power(alice, Capability::Mining, 100), Ok(0));
        assert_eq!(stakes.query_power(alice, Capability::Mining, 101), Ok(100));
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 200),
            Ok(10_000)
        );

        // Let's make Alice stake 50 Wits at epoch 150 this time
        assert_eq!(
            stakes
                .add_stake(alice_charlie, 50, 300, MIN_STAKE_NANOWITS)
                .unwrap(),
            Stake::from_parts(
                150,
                CapabilityMap {
                    mining: 166,
                    witnessing: 166
                },
                2,
            )
        );
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 299),
            Ok(19_950)
        );
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 300),
            Ok(20_100)
        );
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 301),
            Ok(20_250)
        );
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 400),
            Ok(35_100)
        );

        // Now let's make Bob stake 500 Wits at epoch 1000 this time
        assert_eq!(
            stakes
                .add_stake(bob_david, 500, 1_000, MIN_STAKE_NANOWITS)
                .unwrap(),
            Stake::from_parts(
                500,
                CapabilityMap {
                    mining: 1_000,
                    witnessing: 1_000
                },
                1,
            )
        );

        // Before Bob stakes, Alice has all the power
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 999),
            Ok(124950)
        );
        assert_eq!(stakes.query_power(bob, Capability::Mining, 999), Ok(0));

        // New stakes don't change power in the same epoch
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 1_000),
            Ok(125100)
        );
        assert_eq!(stakes.query_power(bob, Capability::Mining, 1_000), Ok(0));

        // Shortly after, Bob's stake starts to gain power
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 1_001),
            Ok(125250)
        );
        assert_eq!(stakes.query_power(bob, Capability::Mining, 1_001), Ok(500));

        // After enough time, Bob overpowers Alice
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 2_000),
            Ok(275_100)
        );
        assert_eq!(
            stakes.query_power(bob, Capability::Mining, 2_000),
            Ok(500_000)
        );
    }

    #[test]
    fn test_coin_age_resets() {
        // First, lets create a setup with a few stakers
        let mut stakes = StakesTester::default();
        let alice = "Alice";
        let bob = "Bob";
        let charlie = "Charlie";
        let david = "David";
        let erin = "Erin";

        let alice_charlie = (alice, charlie);
        let bob_david = (bob, david);
        let charlie_erin = (charlie, erin);

        stakes
            .add_stake(alice_charlie, 10, 0, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(bob_david, 20, 20, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(charlie_erin, 30, 30, MIN_STAKE_NANOWITS)
            .unwrap();

        // Let's really start our test at epoch 100
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 100),
            Ok(1_000)
        );
        assert_eq!(stakes.query_power(bob, Capability::Mining, 100), Ok(1_600));
        assert_eq!(
            stakes.query_power(charlie, Capability::Mining, 100),
            Ok(2_100)
        );
        assert_eq!(
            stakes.query_power(alice, Capability::Witnessing, 100),
            Ok(1_000)
        );
        assert_eq!(
            stakes.query_power(bob, Capability::Witnessing, 100),
            Ok(1_600)
        );
        assert_eq!(
            stakes.query_power(charlie, Capability::Witnessing, 100),
            Ok(2_100)
        );
        assert_eq!(
            stakes.rank(Capability::Mining, 100).collect::<Vec<_>>(),
            [
                (charlie.into(), 2100),
                (bob.into(), 1600),
                (alice.into(), 1000)
            ]
        );
        assert_eq!(
            stakes.rank(Capability::Witnessing, 100).collect::<Vec<_>>(),
            [
                (charlie.into(), 2100),
                (bob.into(), 1600),
                (alice.into(), 1000)
            ]
        );

        // Now let's slash Charlie's mining coin age right after
        stakes
            .reset_age(charlie, Capability::Mining, 101, 1)
            .unwrap();
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 101),
            Ok(1_010)
        );
        assert_eq!(stakes.query_power(bob, Capability::Mining, 101), Ok(1_620));
        assert_eq!(stakes.query_power(charlie, Capability::Mining, 101), Ok(0));
        assert_eq!(
            stakes.query_power(alice, Capability::Witnessing, 101),
            Ok(1_010)
        );
        assert_eq!(
            stakes.query_power(bob, Capability::Witnessing, 101),
            Ok(1_620)
        );
        assert_eq!(
            stakes.query_power(charlie, Capability::Witnessing, 101),
            Ok(2_130)
        );
        assert_eq!(
            stakes.rank(Capability::Mining, 101).collect::<Vec<_>>(),
            [
                (bob.into(), 1_620),
                (alice.into(), 1_010),
                (charlie.into(), 0)
            ]
        );
        assert_eq!(
            stakes.rank(Capability::Witnessing, 101).collect::<Vec<_>>(),
            [
                (charlie.into(), 2_130),
                (bob.into(), 1_620),
                (alice.into(), 1_010)
            ]
        );

        // Don't panic, Charlie! You can start to collect power right after, and eventually, you can
        // even take over again ;)
        assert_eq!(stakes.query_power(charlie, Capability::Mining, 102), Ok(30));
        assert_eq!(
            stakes.query_power(alice, Capability::Mining, 300),
            Ok(3_000)
        );
        assert_eq!(stakes.query_power(bob, Capability::Mining, 300), Ok(5_600));
        assert_eq!(
            stakes.query_power(charlie, Capability::Mining, 300),
            Ok(5_970)
        );
        assert_eq!(
            stakes.query_power(alice, Capability::Witnessing, 300),
            Ok(3_000)
        );
        assert_eq!(
            stakes.query_power(bob, Capability::Witnessing, 300),
            Ok(5_600)
        );
        assert_eq!(
            stakes.query_power(charlie, Capability::Witnessing, 300),
            Ok(8_100)
        );
        assert_eq!(
            stakes.rank(Capability::Mining, 300).collect::<Vec<_>>(),
            [
                (charlie.into(), 5_970),
                (bob.into(), 5_600),
                (alice.into(), 3_000)
            ]
        );
        assert_eq!(
            stakes.rank(Capability::Witnessing, 300).collect::<Vec<_>>(),
            [
                (charlie.into(), 8_100),
                (bob.into(), 5_600),
                (alice.into(), 3_000)
            ]
        );
    }

    #[test]
    fn test_rank_proportional_reset() {
        // First, lets create a setup with a few stakers
        let mut stakes = StakesTester::default();
        let alice = "Alice";
        let bob = "Bob";
        let charlie = "Charlie";
        let david = "David";
        let erin = "Erin";

        let alice_bob = (alice, bob);
        let bob_charlie = (bob, charlie);
        let charlie_david = (charlie, david);
        let david_erin = (david, erin);
        let erin_alice = (erin, alice);

        stakes
            .add_stake(alice_bob, 10, 0, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(bob_charlie, 20, 10, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(charlie_david, 30, 20, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(david_erin, 40, 30, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(erin_alice, 50, 40, MIN_STAKE_NANOWITS)
            .unwrap();

        // Power of validators at epoch 90:
        //      alice_bob:      10 * (90 - 0) = 900
        //      bob_charlie:    20 * (90 - 10) = 1600
        //      charlie_david:  30 * (90 - 20) = 2100
        //      david_erin:     40 * (90 - 30) = 2400
        //      erin_alice:     50 * (90 - 40) = 2500
        let rank_subset: Vec<_> = stakes.rank(Capability::Mining, 90).take(4).collect();
        for (i, (validator, _)) in rank_subset.into_iter().enumerate() {
            let _ = stakes.reset_age(
                validator,
                Capability::Mining,
                90,
                (i + 1).try_into().unwrap(),
            );
        }

        // Slashed with a factor 1 / 1
        assert_eq!(stakes.query_power(erin, Capability::Mining, 90), Ok(0));
        // Slashed with a factor 1 / 2
        assert_eq!(stakes.query_power(david, Capability::Mining, 90), Ok(1200));
        // Slashed with a factor 1 / 3
        assert_eq!(
            stakes.query_power(charlie, Capability::Mining, 90),
            Ok(1410)
        );
        // Slashed with a factor 1 / 4
        assert_eq!(stakes.query_power(bob, Capability::Mining, 90), Ok(1200));
        // Not slashed
        assert_eq!(stakes.query_power(alice, Capability::Mining, 90), Ok(900));
    }

    #[test]
    fn test_query_stakes() {
        // First, lets create a setup with a few stakers
        let mut stakes = StakesTester::default();
        let alice = "Alice";
        let bob = "Bob";
        let charlie = "Charlie";
        let david = "David";
        let erin = "Erin";

        let alice_charlie = (alice, charlie);
        let bob_david = (bob, david);
        let charlie_erin = (charlie, erin);

        stakes
            .add_stake(alice_charlie, 10, 0, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(bob_david, 20, 30, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(charlie_erin, 40, 50, MIN_STAKE_NANOWITS)
            .unwrap();

        let result = stakes.query_stakes(QueryStakesKey::Key(bob_david.into()));
        assert_eq!(
            result,
            Ok(vec![StakeEntry {
                key: bob_david.into(),
                value: Stake::from_parts(
                    20,
                    CapabilityMap {
                        mining: 30,
                        witnessing: 30
                    },
                    1,
                )
            }])
        );

        let result = stakes.query_by_validator(bob.into());
        assert_eq!(
            result,
            Ok(vec![StakeEntry {
                key: bob_david.into(),
                value: Stake::from_parts(
                    20,
                    CapabilityMap {
                        mining: 30,
                        witnessing: 30
                    },
                    1,
                )
            }])
        );

        let result = stakes.query_by_validator(david.into());
        assert_eq!(
            result,
            Err(StakesError::ValidatorNotFound {
                validator: david.into()
            })
        );

        let result = stakes.query_by_withdrawer(david.into());
        assert_eq!(
            result,
            Ok(vec![StakeEntry {
                key: bob_david.into(),
                value: Stake::from_parts(
                    20,
                    CapabilityMap {
                        mining: 30,
                        witnessing: 30
                    },
                    1,
                )
            }])
        );

        let result = stakes.query_by_withdrawer(bob.into());
        assert_eq!(
            result,
            Err(StakesError::WithdrawerNotFound {
                withdrawer: bob.into()
            })
        );
    }

    #[test]
    fn test_serde() {
        use bincode;

        let mut stakes = StakesTester::default();
        let alice = String::from("Alice");
        let bob = String::from("Bob");

        let alice_bob = (alice.clone(), bob.clone());
        stakes
            .add_stake(alice_bob, 123, 456, MIN_STAKE_NANOWITS)
            .ok();

        let serialized = bincode::serialize(&stakes).unwrap().clone();
        let mut deserialized: StakesTester = bincode::deserialize(serialized.as_slice()).unwrap();

        deserialized
            .reset_age(alice.clone(), Capability::Mining, 789, 1)
            .ok();
        deserialized.query_by_validator(alice).unwrap();

        let epoch = deserialized.query_by_withdrawer(bob.clone()).unwrap()[0]
            .value
            .epochs
            .mining;

        assert_eq!(epoch, 789);
    }

    #[test]
    fn test_validator_withdrawer_pair() {
        // First, lets create a setup with a few stakers
        let mut stakes = StakesTester::default();
        let alice = "Alice";
        let bob = "Bob";
        let charlie = "Charlie";

        // Validator not used yet, so we can stake with any (validator, withdrawer) pair
        assert_eq!(stakes.check_validator_withdrawer(alice, bob), Ok(()));
        assert_eq!(stakes.check_validator_withdrawer(alice, charlie), Ok(()));

        // Use the validator with a (validator, withdrawer) pair
        stakes
            .add_stake((alice, bob), 10, 0, MIN_STAKE_NANOWITS)
            .unwrap();

        // The validator is used, we can still stake as long as the correct withdrawer is used
        assert_eq!(stakes.check_validator_withdrawer(alice, bob), Ok(()));

        // Validator used with another withdrawer address, throw an error
        let valid_pair = stakes.check_validator_withdrawer(alice, charlie);
        assert_eq!(
            valid_pair,
            Err(StakesError::DifferentWithdrawer {
                validator: alice.into()
            })
        );
    }

    #[test]
    fn test_stakes_nonce() {
        // First, lets create a setup with a few stakers
        let mut stakes = StakesTester::default();
        let alice = "Alice";
        let bob = "Bob";
        let charlie = "Charlie";
        let david = "David";

        let alice_charlie = (alice, charlie);
        let bob_david = (bob, david);

        stakes
            .add_stake(alice_charlie, 10, 0, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(bob_david, 20, 10, MIN_STAKE_NANOWITS)
            .unwrap();
        assert_eq!(stakes.query_nonce(alice_charlie), Ok(1));
        assert_eq!(stakes.query_nonce(bob_david), Ok(1));

        stakes
            .remove_stake(bob_david, 10, MIN_STAKE_NANOWITS)
            .unwrap();
        assert_eq!(stakes.query_nonce(alice_charlie), Ok(1));
        assert_eq!(stakes.query_nonce(bob_david), Ok(2));

        stakes
            .add_stake(bob_david, 40, 30, MIN_STAKE_NANOWITS)
            .unwrap();
        assert_eq!(stakes.query_nonce(alice_charlie), Ok(1));
        assert_eq!(stakes.query_nonce(bob_david), Ok(3));
    }
}
