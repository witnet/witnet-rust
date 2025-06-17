use std::{
    collections::BTreeMap,
    fmt::{Debug, Display, Formatter},
    iter::Sum,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, Mul, Rem, Sub},
    rc::Rc,
    str::FromStr,
    sync::RwLock,
};

use anyhow::Error;
use num_traits::{Saturating, Zero};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{DeserializeOwned, MapAccess, Visitor},
};

use crate::{
    chain::PublicKeyHash, proto::ProtobufConvert, staking::prelude::*, wit::PrecisionLoss,
};

/// Just a type alias for consistency of using the same data type to represent power.
pub type Power = u64;

/// The resulting type for all the fallible functions in this module.
pub type StakesResult<T, Address, Coins, Epoch> = Result<T, StakesError<Address, Coins, Epoch>>;
/// Pairs a stake key and the stake data it refers.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct StakeEntry<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default + Serialize,
    Coins: Clone + Serialize,
    Epoch: Clone + Default + Serialize,
    Nonce: Clone + Default + Serialize,
    Power: Clone + Serialize,
{
    /// The key to which this stake entry belongs to.
    pub key: StakeKey<Address>,
    /// The stake data itself.
    pub value: Stake<UNIT, Address, Coins, Epoch, Nonce, Power>,
}

/// The resulting type for functions in this module that return a single stake entry.
pub type StakeEntryResult<const UNIT: u8, Address, Coins, Epoch, Nonce, Power> =
    StakesResult<StakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>, Address, Coins, Epoch>;

/// The resulting type for functions in this module that return a vector of stake entries.
pub type StakeEntryVecResult<const UNIT: u8, Address, Coins, Epoch, Nonce, Power> =
    StakesResult<Vec<StakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>>, Address, Coins, Epoch>;

impl<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
    From<StakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>>
    for Stake<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default + Serialize,
    Coins: Clone + Serialize,
    Epoch: Clone + Default + Serialize,
    Nonce: Clone + Default + Serialize,
    Power: Clone + Serialize,
{
    fn from(entry: StakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>) -> Self {
        entry.value
    }
}

/// A reference-counted and read-write-locked equivalent to `StakeEntry`.
///
/// This is needed for implementing `PartialEq` manually on the locked data, which cannot be done directly
/// because those are externally owned types.
#[allow(clippy::type_complexity)]
#[derive(Clone, Debug, Default)]
pub struct SyncStakeEntry<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default,
    Coins: Clone,
    Epoch: Clone + Default,
    Nonce: Clone + Default,
    Power: Clone,
{
    /// A smart lock referring the key to which this stake entry belongs to.
    pub key: Rc<RwLock<StakeKey<Address>>>,
    /// A smart lock referring the stake data itself.
    pub value: Rc<RwLock<Stake<UNIT, Address, Coins, Epoch, Nonce, Power>>>,
}

impl<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
    SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default + Serialize,
    Coins: Clone + Serialize,
    Epoch: Clone + Default + Serialize,
    Nonce: Clone + Default + Serialize,
    Power: Clone + Serialize,
{
    /// Locks and reads both the stake key and the stake data referred by the stake entry.
    pub fn read_entry(&self) -> StakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power> {
        let key = self.read_key();
        let value = self.read_value();

        StakeEntry { key, value }
    }

    /// Locks and reads the stake key referred by the stake entry.
    #[inline]
    pub fn read_key(&self) -> StakeKey<Address> {
        self.key.read().unwrap().clone()
    }

    /// Locks and reads the stake data referred by the stake entry.
    #[inline]
    pub fn read_value(&self) -> Stake<UNIT, Address, Coins, Epoch, Nonce, Power> {
        self.value.read().unwrap().clone()
    }
}

impl<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
    From<StakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>>
    for SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default + Serialize,
    Coins: Clone + Serialize,
    Epoch: Clone + Default + Serialize,
    Nonce: Clone + Default + Serialize,
    Power: Clone + Serialize,
{
    fn from(entry: StakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>) -> Self {
        SyncStakeEntry {
            key: Rc::new(RwLock::new(entry.key)),
            value: Rc::new(RwLock::new(entry.value)),
        }
    }
}

impl<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
    From<&SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>>
    for StakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default + Serialize,
    Coins: Clone + Serialize,
    Epoch: Clone + Default + Serialize,
    Nonce: Clone + Default + Serialize,
    Power: Clone + Serialize,
{
    fn from(sync: &SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>) -> Self {
        StakeEntry {
            key: sync.read_key(),
            value: sync.read_value(),
        }
    }
}

impl<const UNIT: u8, Address, Coins, Epoch, Nonce, Power> PartialEq
    for SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default,
    Coins: Clone + PartialEq,
    Epoch: Clone + Default + PartialEq,
    Nonce: Clone + Default + PartialEq,
    Power: Clone,
{
    fn eq(&self, other: &Self) -> bool {
        let self_stake = self.value.read().unwrap();
        let other_stake = other.value.read().unwrap();

        self_stake.coins.eq(&other_stake.coins)
            && other_stake.epochs.eq(&other_stake.epochs)
            && other_stake.nonce.eq(&other_stake.nonce)
    }
}

impl<'de, const UNIT: u8, Address, Coins, Epoch, Nonce, Power> Deserialize<'de>
    for SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default + Serialize,
    Coins: Clone + Serialize,
    Epoch: Clone + Default + Serialize,
    Nonce: Clone + Default + Serialize,
    Power: Clone + Serialize,
    StakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>: Deserialize<'de>,
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        <StakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>>::deserialize(deserializer)
            .map(SyncStakeEntry::from)
    }
}

impl<const UNIT: u8, Address, Coins, Epoch, Nonce, Power> Serialize
    for SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default + Serialize,
    Coins: Clone + Serialize,
    Epoch: Clone + Default + Serialize,
    Nonce: Clone + Default + Serialize,
    Power: Clone + Serialize,
    Stake<UNIT, Address, Coins, Epoch, Nonce, Power>: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        StakeEntry::from(self).serialize(serializer)
    }
}

/// Couples a validator address with a withdrawer address together. This is meant to be used in `Stakes` as the index
/// for the `by_key` index.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct StakeKey<Address> {
    /// A validator address.
    pub validator: Address,
    /// A withdrawer address.
    pub withdrawer: Address,
}

impl ProtobufConvert for StakeKey<PublicKeyHash> {
    type ProtoStruct = crate::proto::schema::witnet::StakeKey;

    fn to_pb(&self) -> Self::ProtoStruct {
        let mut proto = Self::ProtoStruct::new();
        proto.set_validator(self.validator.to_pb());
        proto.set_withdrawer(self.withdrawer.to_pb());

        proto
    }

    fn from_pb(mut pb: Self::ProtoStruct) -> Result<Self, Error> {
        let validator = PublicKeyHash::from_pb(pb.take_validator())?;
        let withdrawer = PublicKeyHash::from_pb(pb.take_withdrawer())?;

        Ok(Self {
            validator,
            withdrawer,
        })
    }
}

impl<Address, T> From<(T, T)> for StakeKey<Address>
where
    T: Into<Address>,
{
    fn from(val: (T, T)) -> Self {
        StakeKey {
            validator: val.0.into(),
            withdrawer: val.1.into(),
        }
    }
}

impl<Address> From<&str> for StakeKey<Address>
where
    Address: FromStr,
    <Address as FromStr>::Err: std::fmt::Debug,
{
    fn from(val: &str) -> Self {
        StakeKey {
            validator: Address::from_str(val).unwrap(),
            withdrawer: Address::from_str(val).unwrap(),
        }
    }
}

impl<Address> Display for StakeKey<Address>
where
    Address: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "validator: {} withdrawer: {}",
            self.validator, self.withdrawer
        )
    }
}

/// Couples an amount of coins, a validator address and a withdrawer address together. This is meant to be used in
/// `Stakes` as the index of the `by_coins` index.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct CoinsAndAddresses<Coins, Address> {
    /// An amount of coins.
    pub coins: Coins,
    /// A validator and withdrawer addresses pair.
    pub addresses: StakeKey<Address>,
}

/// Allows telling the `census` method in `Stakes` to source addresses from its internal `by_coins`
/// following different strategies.
#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum CensusStrategy {
    /// Retrieve all addresses, ordered by decreasing power.
    All = 0,
    /// Retrieve every Nth address, ordered by decreasing power.
    StepBy(usize) = 1,
    /// Retrieve the most powerful N addresses, ordered by decreasing power.
    Take(usize) = 2,
    /// Retrieve a total of N addresses, evenly distributed from the index, ordered by decreasing
    /// power.
    Evenly(usize) = 3,
}

impl<const UNIT: u8, Address, Coins, Epoch, Nonce, Power> Serialize
    for Stakes<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Default + Ord,
    Coins: Clone + Ord,
    Epoch: Clone + Default,
    Nonce: Clone + Default,
    Power: Clone,
    StakeKey<Address>: Serialize,
    SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.by_key.serialize(serializer)
    }
}

impl<'de, const UNIT: u8, Address, Coins, Epoch, Nonce, Power> Deserialize<'de>
    for Stakes<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone
        + Debug
        + Default
        + DeserializeOwned
        + Display
        + Ord
        + Send
        + Serialize
        + Sync
        + 'static,
    Coins: Copy
        + Debug
        + Default
        + Display
        + DeserializeOwned
        + From<u64>
        + Mul<Output = Coins>
        + Mul<Epoch, Output = Power>
        + Ord
        + PrecisionLoss
        + Send
        + Serialize
        + Sub<Output = Coins>
        + Sum
        + Sync
        + Zero
        + Div<Output = Coins>
        + Rem<Output = Coins>,
    Epoch: Copy
        + Debug
        + Default
        + DeserializeOwned
        + Display
        + From<u32>
        + Saturating
        + Send
        + Serialize
        + Sub<Output = Epoch>
        + Sync
        + Add<Output = Epoch>
        + Div<Output = Epoch>
        + PartialOrd,
    Nonce: AddAssign
        + Copy
        + Debug
        + Default
        + DeserializeOwned
        + Display
        + From<Epoch>
        + From<u32>
        + Saturating
        + Send
        + Serialize
        + Sync,
    Power: Add<Output = Power>
        + Copy
        + Default
        + DeserializeOwned
        + Div<Output = Power>
        + Ord
        + Serialize
        + Sum,
    u64: From<Coins> + From<Power>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer
            .deserialize_map(StakesVisitor::<UNIT, Address, Coins, Epoch, Nonce, Power>::default())
    }
}

#[derive(Default)]
struct StakesVisitor<const UNIT: u8, Address, Coins, Epoch, Nonce, Power> {
    phantom_address: PhantomData<Address>,
    phantom_coins: PhantomData<Coins>,
    phantom_epoch: PhantomData<Epoch>,
    phantom_action: PhantomData<Nonce>,
    phantom_power: PhantomData<Power>,
}

impl<'de, const UNIT: u8, Address, Coins, Epoch, Nonce, Power> Visitor<'de>
    for StakesVisitor<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone
        + Debug
        + Default
        + Deserialize<'de>
        + Display
        + Ord
        + Send
        + Serialize
        + Sync
        + 'static,
    Coins: Copy
        + Debug
        + Default
        + Deserialize<'de>
        + Display
        + From<u64>
        + Mul<Output = Coins>
        + Mul<Epoch, Output = Power>
        + Ord
        + PrecisionLoss
        + Send
        + Serialize
        + Sub<Output = Coins>
        + Sum
        + Sync
        + Zero
        + Div<Output = Coins>
        + Rem<Output = Coins>,
    Epoch: Copy
        + Debug
        + Default
        + Deserialize<'de>
        + Display
        + From<u32>
        + Send
        + Serialize
        + Saturating
        + Sub<Output = Epoch>
        + Sync
        + Add<Output = Epoch>
        + Div<Output = Epoch>
        + PartialOrd,
    Nonce: AddAssign
        + Copy
        + Debug
        + Default
        + DeserializeOwned
        + Display
        + From<Epoch>
        + From<u32>
        + Saturating
        + Send
        + Serialize
        + Sync,
    Power: Add<Output = Power>
        + Copy
        + Default
        + Deserialize<'de>
        + Div<Output = Power>
        + Ord
        + Serialize
        + Sum,
    u64: From<Coins> + From<Power>,
{
    type Value = Stakes<UNIT, Address, Coins, Epoch, Nonce, Power>;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("Stakes<Address, Coins, Epoch, Power>")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut entries = <BTreeMap<
            StakeKey<Address>,
            SyncStakeEntry<UNIT, Address, Coins, Epoch, Nonce, Power>,
        >>::new();

        while let Some((key, value)) = map.next_entry()? {
            entries.insert(key, value);
        }

        let stakes = Stakes::with_entries(entries);

        Ok(stakes)
    }
}

/// Tells the stakes tracker what to do with the nonce associated to the entry or entries being
/// updated.
///
/// This allows customizing the behavior of the nonce to be different when updating a stake entry
/// when processing a stake or unstake transaction vs. when adding rewards or enforcing slashing.
///
/// Generally speaking, we want to update the nonce when we are processing a stake or unstake
/// transaction, but we want to keep the nonce the same if it is a reward or slashing act.
#[derive(Debug, PartialEq)]
pub enum NoncePolicy<Epoch> {
    /// Update the value of the nonce field by deriving it from this epoch.
    SetFromEpoch(Epoch),
    /// Leave the value of the nonce field as is.
    KeepAsIs,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_cloning_assumptions() {
        let a = SyncStakeEntry::<0, String, u64, u64, u64, u64>::from(StakeEntry {
            key: Default::default(),
            value: Stake::from_parts(123, Default::default(), Default::default()),
        });
        let b = a.clone();

        {
            let mut value = b.value.write().unwrap();
            value.coins = 456;
        }

        assert_eq!(a.value.read().unwrap().coins, 456);
    }
}
