use std::{rc::Rc, str::FromStr, sync::RwLock};

use super::prelude::*;

/// Type alias for a reference-counted and read-write-locked instance of `Stake`.
pub type SyncStake<Address, Coins, Epoch, Power> = Rc<RwLock<Stake<Address, Coins, Epoch, Power>>>;

/// The resulting type for all the fallible functions in this module.
pub type Result<T, Address, Coins, Epoch> =
    std::result::Result<T, StakesError<Address, Coins, Epoch>>;

/// Couples a validator address with a withdrawer address together. This is meant to be used in `Stakes` as the index
/// for the `by_key` index.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StakeKey<Address> {
    /// A validator address.
    pub validator: Address,
    /// A withdrawer address.
    pub withdrawer: Address,
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

/// Couples an amount of coins, a validator address and a withdrawer address together. This is meant to be used in
/// `Stakes` as the index of the `by_coins` index.
#[derive(Eq, Ord, PartialEq, PartialOrd)]
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
