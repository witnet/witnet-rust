use std::fmt::{Debug, Display, Formatter};
use std::{rc::Rc, str::FromStr, sync::RwLock};

use failure::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{chain::PublicKeyHash, proto::ProtobufConvert};

use crate::staking::prelude::*;

/// Just a type alias for consistency of using the same data type to represent power.
pub type Power = u64;

/// The resulting type for all the fallible functions in this module.
pub type StakesResult<T, Address, Coins, Epoch> = Result<T, StakesError<Address, Coins, Epoch>>;

/// Newtype for a reference-counted and read-write-locked instance of `Stake`.
///
/// This newtype is needed for implementing `PartialEq` manually on the locked data, which cannot be done directly
/// because those are externally owned types.
#[derive(Clone, Debug, Default)]
pub struct SyncStake<Address, Coins, Epoch, Power>
where
    Address: Default,
    Epoch: Default,
{
    /// The lock itself.
    pub value: Rc<RwLock<Stake<Address, Coins, Epoch, Power>>>,
}

impl<Address, Coins, Epoch, Power> From<Stake<Address, Coins, Epoch, Power>>
    for SyncStake<Address, Coins, Epoch, Power>
where
    Address: Default,
    Epoch: Default,
{
    #[inline]
    fn from(value: Stake<Address, Coins, Epoch, Power>) -> Self {
        SyncStake {
            value: Rc::new(RwLock::new(value)),
        }
    }
}

impl<Address, Coins, Epoch, Power> PartialEq for SyncStake<Address, Coins, Epoch, Power>
where
    Address: Default,
    Epoch: Default + PartialEq,
    Coins: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        let self_stake = self.value.read().unwrap();
        let other_stake = other.value.read().unwrap();

        self_stake.coins.eq(&other_stake.coins) && other_stake.epochs.eq(&other_stake.epochs)
    }
}

impl<'de, Address, Coins, Epoch, Power> Deserialize<'de> for SyncStake<Address, Coins, Epoch, Power>
where
    Address: Default,
    Epoch: Default,
    Stake<Address, Coins, Epoch, Power>: Deserialize<'de>,
{
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        <Stake<Address, Coins, Epoch, Power>>::deserialize(deserializer).map(SyncStake::from)
    }
}

impl<Address, Coins, Epoch, Power> Serialize for SyncStake<Address, Coins, Epoch, Power>
where
    Address: Default,
    Epoch: Default,
    Stake<Address, Coins, Epoch, Power>: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.value.read().unwrap().serialize(serializer)
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
