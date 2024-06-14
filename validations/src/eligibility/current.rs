use std::{
    fmt::{Debug, Display},
    iter::Sum,
    ops::{Add, Div, Mul, Sub},
};

use witnet_data_structures::{staking::prelude::*, wit::PrecisionLoss};

const MINING_REPLICATION_FACTOR: usize = 4;
const WITNESSING_MAX_ROUNDS: usize = 4;

/// Different reasons for ineligibility of a validator, stake entry or eligibility proof.
#[derive(Copy, Debug, Clone, PartialEq)]
pub enum IneligibilityReason {
    /// The stake entry has no power enough to perform such action.
    InsufficientPower,
    /// No matching stake entry has been found.
    NotStaking,
}

/// Signals whether a validator, stake entry or eligibility proof is eligible or not, and in the negative case, it also
/// provides a reason for ineligibility.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Eligible {
    /// It is eligible.
    Yes,
    /// It is not eligible (provides a reason).
    No(IneligibilityReason),
}

impl From<IneligibilityReason> for Eligible {
    #[inline]
    fn from(reason: IneligibilityReason) -> Self {
        Eligible::No(reason)
    }
}

/// Trait providing eligibility calculation for multiple protocol capabilities.
pub trait Eligibility<Address, Coins, Epoch, Power>
where
    Address: Debug + Display + Sync + Send + 'static,
    Coins: Debug + Display + Sync + Send + Sum + 'static,
    Epoch: Debug + Display + Sync + Send + 'static,
{
    /// Tells whether a VRF proof meets the requirements to become eligible for mining. Unless an error occurs, returns
    /// an `Eligibility` structure signaling eligibility or lack thereof (in which case you also get an
    /// `IneligibilityReason`.
    fn mining_eligibility<ISK>(
        &self,
        validator: ISK,
        epoch: Epoch,
    ) -> StakesResult<Eligible, Address, Coins, Epoch>
    where
        ISK: Into<Address>;

    /// Tells whether a VRF proof meets the requirements to become eligible for mining. Because this function returns a
    /// simple `bool`, it is best-effort: both lack of eligibility and any error cases are mapped to `false`.
    fn mining_eligibility_bool<ISK>(&self, validator: ISK, epoch: Epoch) -> bool
    where
        ISK: Into<Address>,
    {
        matches!(self.mining_eligibility(validator, epoch), Ok(Eligible::Yes))
    }

    /// Tells whether a VRF proof meets the requirements to become eligible for witnessing. Unless an error occurs,
    /// returns an `Eligibility` structure signaling eligibility or lack thereof (in which case you also get an
    /// `IneligibilityReason`.
    fn witnessing_eligibility<ISK>(
        &self,
        validator: ISK,
        epoch: Epoch,
        witnesses: u8,
        round: u8,
    ) -> StakesResult<Eligible, Address, Coins, Epoch>
    where
        ISK: Into<Address>;

    /// Tells whether a VRF proof meets the requirements to become eligible for witnessing. Because this function
    /// returns a simple `bool`, it is best-effort: both lack of eligibility and any error cases are mapped to `false`.
    fn witnessing_eligibility_bool<ISK>(
        &self,
        validator: ISK,
        epoch: Epoch,
        witnesses: u8,
        round: u8,
    ) -> bool
    where
        ISK: Into<Address>,
    {
        matches!(
            self.witnessing_eligibility(validator, epoch, witnesses, round),
            Ok(Eligible::Yes)
        )
    }
}

impl<Address, Coins, Epoch, Power> Eligibility<Address, Coins, Epoch, Power>
    for Stakes<Address, Coins, Epoch, Power>
where
    Address: Clone + Debug + Default + Display + Ord + Sync + Send + 'static,
    Coins: Copy
        + Debug
        + Default
        + Display
        + Ord
        + From<u64>
        + Into<u64>
        + num_traits::Zero
        + Add<Output = Coins>
        + Sub<Output = Coins>
        + Mul
        + Mul<Epoch, Output = Power>
        + PrecisionLoss
        + Sync
        + Send
        + Sum
        + 'static,
    Epoch: Copy
        + Debug
        + Default
        + Display
        + num_traits::Saturating
        + Sub<Output = Epoch>
        + Add<Output = Epoch>
        + Div<Output = Epoch>
        + From<u32>
        + Sync
        + Send
        + 'static,
    Power: Copy
        + Default
        + Ord
        + Add<Output = Power>
        + Sub<Output = Power>
        + Mul<Output = Power>
        + Div<Output = Power>
        + From<u64>
        + Sum
        + Display,
    u64: From<Coins> + From<Power>,
{
    fn mining_eligibility<ISK>(
        &self,
        validator: ISK,
        epoch: Epoch,
    ) -> StakesResult<Eligible, Address, Coins, Epoch>
    where
        ISK: Into<Address>,
    {
        let power = match self.query_power(validator, Capability::Mining, epoch) {
            Ok(p) => p,
            Err(e) => {
                // Early exit if the stake key does not exist
                return match e {
                    StakesError::EntryNotFound { .. } => Ok(IneligibilityReason::NotStaking.into()),
                    e => Err(e),
                };
            }
        };

        // Requirement no. 2 from the WIP:
        //  "the mining power of the block proposer is in the `rf / stakers`th quantile among the mining powers of all
        //  the stakers"
        // TODO: verify if defaulting to 0 makes sense
        let mut rank = self.rank(Capability::Mining, epoch);
        let (_, threshold) = rank.nth(MINING_REPLICATION_FACTOR - 1).unwrap_or_default();
        if power < threshold {
            return Ok(IneligibilityReason::InsufficientPower.into());
        }

        // If all the requirements are met, we can deem it as eligible
        Ok(Eligible::Yes)
    }

    fn witnessing_eligibility<ISK>(
        &self,
        key: ISK,
        epoch: Epoch,
        witnesses: u8,
        round: u8,
    ) -> StakesResult<Eligible, Address, Coins, Epoch>
    where
        ISK: Into<Address>,
    {
        let power = match self.query_power(key, Capability::Witnessing, epoch) {
            Ok(p) => p,
            Err(e) => {
                // Early exit if the stake key does not exist
                return match e {
                    StakesError::EntryNotFound { .. } => Ok(IneligibilityReason::NotStaking.into()),
                    e => Err(e),
                };
            }
        };

        let mut rank = self.rank(Capability::Mining, epoch);
        let rf = 2usize.pow(u32::from(round)) * witnesses as usize;

        // Requirement no. 2 from the WIP:
        //  "the witnessing power of the block proposer is in the `rf / stakers`th quantile among the witnessing powers
        //  of all the stakers"
        let stakers = self.stakes_count();
        let quantile = stakers / MINING_REPLICATION_FACTOR;
        // TODO: verify if defaulting to 0 makes sense
        let (_, threshold) = rank.nth(quantile).unwrap_or_default();
        if power <= threshold {
            return Ok(IneligibilityReason::InsufficientPower.into());
        }

        // Requirement no. 3 from the WIP:
        //  "the big-endian value of the VRF output is less than
        //  `max_rounds * own_power / (max_power * (rf - max_rounds) - rf *  threshold_power)`"
        // TODO: verify if defaulting to 0 makes sense
        let (_, max_power) = rank.next().unwrap_or_default();
        let stakers = self.stakes_count();
        let quantile = stakers / rf;
        // TODO: verify if defaulting to 0 makes sense
        let (_, threshold_power) = rank.nth(quantile).unwrap_or_default();
        let dividend = Power::from(WITNESSING_MAX_ROUNDS as u64) * power;
        let divisor = max_power * Power::from((rf - WITNESSING_MAX_ROUNDS) as u64)
            - Power::from(rf as u64) * threshold_power;
        let threshold = dividend / divisor;
        println!("{}", u64::from(power));
        println!("{}", u64::from(threshold));
        if power <= threshold {
            return Ok(IneligibilityReason::InsufficientPower.into());
        }

        Ok(Eligible::Yes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mining_eligibility_no_stakers() {
        let stakes = <Stakes<String, _, _, _>>::with_minimum(100u64);
        let isk = "validator";

        assert_eq!(
            stakes.mining_eligibility(isk, 0),
            Ok(Eligible::No(IneligibilityReason::NotStaking))
        );
        assert!(!stakes.mining_eligibility_bool(isk, 0));

        assert_eq!(
            stakes.mining_eligibility(isk, 100),
            Ok(Eligible::No(IneligibilityReason::NotStaking))
        );
        assert!(!stakes.mining_eligibility_bool(isk, 100));
    }

    #[test]
    fn test_mining_eligibility_absolute_power() {
        let mut stakes = <Stakes<String, _, _, _>>::with_minimum(100u64);
        let isk = "validator";

        stakes.add_stake(isk, 1_000, 0).unwrap();

        assert_eq!(
            stakes.mining_eligibility(isk, 0),
            Ok(Eligible::No(IneligibilityReason::InsufficientPower))
        );
        assert!(!stakes.mining_eligibility_bool(isk, 0));

        assert_eq!(stakes.mining_eligibility(isk, 100), Ok(Eligible::Yes));
        assert!(stakes.mining_eligibility_bool(isk, 100));
    }

    #[test]
    fn test_witnessing_eligibility_no_stakers() {
        let stakes = <Stakes<String, _, _, _>>::with_minimum(100u64);
        let isk = "validator";

        assert_eq!(
            stakes.witnessing_eligibility(isk, 0, 10, 0),
            Ok(Eligible::No(IneligibilityReason::NotStaking))
        );
        assert!(!stakes.witnessing_eligibility_bool(isk, 0, 10, 0));

        assert_eq!(
            stakes.witnessing_eligibility(isk, 100, 10, 0),
            Ok(Eligible::No(IneligibilityReason::NotStaking))
        );
        assert!(!stakes.witnessing_eligibility_bool(isk, 100, 10, 0));
    }

    #[test]
    fn test_witnessing_eligibility_absolute_power() {
        let mut stakes = <Stakes<String, _, _, _>>::with_minimum(100u64);
        let isk = "validator";

        stakes.add_stake(isk, 1_000, 0).unwrap();

        assert_eq!(
            stakes.witnessing_eligibility(isk, 0, 10, 0),
            Ok(Eligible::No(IneligibilityReason::InsufficientPower))
        );
        assert!(!stakes.witnessing_eligibility_bool(isk, 0, 10, 0));

        assert_eq!(
            stakes.witnessing_eligibility(isk, 100, 10, 0),
            Ok(Eligible::Yes)
        );
        assert!(stakes.witnessing_eligibility_bool(isk, 100, 10, 0));
    }
}
