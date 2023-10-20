use std::rc::Rc;
use std::sync::RwLock;

use super::prelude::*;

/// Type alias for a reference-counted and read-write-locked instance of `Stake`.
pub type SyncStake<Address, Coins, Epoch, Power> = Rc<RwLock<Stake<Address, Coins, Epoch, Power>>>;

/// The resulting type for all the fallible functions in this module.
pub type Result<T, Address, Coins, Epoch> =
    std::result::Result<T, StakesError<Address, Coins, Epoch>>;

/// Couples an amount of coins and an address together. This is to be used in `Stakes` as the index
/// of the `by_coins` index..
#[derive(Eq, Ord, PartialEq, PartialOrd)]
pub struct CoinsAndAddress<Coins, Address> {
    /// An amount of coins.
    pub coins: Coins,
    /// The address of a staker.
    pub address: Address,
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
