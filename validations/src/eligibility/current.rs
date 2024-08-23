use std::{
    fmt::{Debug, Display},
    iter::Sum,
    ops::{Add, Div, Mul, Rem, Sub},
};

use witnet_data_structures::{chain::Hash, staking::prelude::*, wit::PrecisionLoss};

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
        witnesses: u16,
        round: u16,
    ) -> StakesResult<(Eligible, Hash, f64), Address, Coins, Epoch>
    where
        ISK: Into<Address>;

    /// Tells whether a VRF proof meets the requirements to become eligible for witnessing. Because this function
    /// returns a simple `bool`, it is best-effort: both lack of eligibility and any error cases are mapped to `false`.
    fn witnessing_eligibility_bool<ISK>(
        &self,
        validator: ISK,
        epoch: Epoch,
        witnesses: u16,
        round: u16,
    ) -> bool
    where
        ISK: Into<Address>,
    {
        match self.witnessing_eligibility(validator, epoch, witnesses, round) {
            Ok((eligible, _, _)) => matches!(eligible, Eligible::Yes),
            Err(_) => false,
        }
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
        + Div<Output = Coins>
        + Rem<Output = Coins>
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
    u64: From<Coins> + From<Power> + Mul<Power, Output = u64> + Div<Power, Output = u64>,
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
        witnesses: u16,
        round: u16,
    ) -> StakesResult<(Eligible, Hash, f64), Address, Coins, Epoch>
    where
        ISK: Into<Address>,
    {
        let power = match self.query_power(key, Capability::Witnessing, epoch) {
            Ok(p) => p,
            Err(e) => {
                // Early exit if the stake key does not exist
                return match e {
                    StakesError::EntryNotFound { .. } => {
                        Ok((IneligibilityReason::NotStaking.into(), Hash::min(), 0.0))
                    }
                    e => Err(e),
                };
            }
        };

        let mut rank = self.rank(Capability::Witnessing, epoch);
        let (_, max_power) = rank.next().unwrap_or_default();

        // Requirement no. 2 from the WIP:
        //  "the mining power of the block proposer is in the `rf / stakers`th quantile among the witnessing powers of all
        //  the stakers"
        let rf = 2usize.pow(u32::from(round)) * witnesses as usize;
        let (_, threshold_power) = rank.nth(rf - 2).unwrap_or_default();
        if power <= threshold_power {
            return Ok((
                IneligibilityReason::InsufficientPower.into(),
                Hash::min(),
                0.0,
            ));
        }

        // Requirement no. 3 from the WIP:
        //  "the big-endian value of the VRF output is less than
        //  `max_rounds * own_power / (max_power * (rf - max_rounds) - rf * threshold_power)`"
        let dividend = Power::from(WITNESSING_MAX_ROUNDS as u64) * power;
        let divisor = max_power * Power::from((rf - WITNESSING_MAX_ROUNDS) as u64)
            - Power::from(rf as u64) * threshold_power;
        let target_hash = if divisor == Power::from(0) {
            Hash::with_first_u32(u32::MAX)
        } else {
            Hash::with_first_u32(
                (((u64::MAX / divisor).saturating_mul(dividend.into())) >> 32)
                    .try_into()
                    .unwrap(),
            )
        };

        Ok((
            Eligible::Yes,
            target_hash,
            (u64::from(dividend) / divisor) as f64,
        ))
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

        match stakes.witnessing_eligibility(isk, 0, 10, 0) {
            Ok((eligible, _, _)) => {
                assert_eq!(eligible, Eligible::No(IneligibilityReason::NotStaking));
            }
            Err(_) => assert!(false),
        }
        assert!(!stakes.witnessing_eligibility_bool(isk, 0, 10, 0));

        match stakes.witnessing_eligibility(isk, 100, 10, 0) {
            Ok((eligible, _, _)) => {
                assert_eq!(eligible, Eligible::No(IneligibilityReason::NotStaking));
            }
            Err(_) => assert!(false),
        }
        assert!(!stakes.witnessing_eligibility_bool(isk, 100, 10, 0));
    }

    #[test]
    fn test_witnessing_eligibility_absolute_power() {
        let mut stakes = <Stakes<String, _, _, _>>::with_minimum(100u64);
        let isk = "validator";

        stakes.add_stake(isk, 1_000, 0).unwrap();

        match stakes.witnessing_eligibility(isk, 0, 10, 0) {
            Ok((eligible, _, _)) => {
                assert_eq!(
                    eligible,
                    Eligible::No(IneligibilityReason::InsufficientPower)
                );
            }
            Err(_) => assert!(false),
        }
        assert!(!stakes.witnessing_eligibility_bool(isk, 0, 10, 0));

        match stakes.witnessing_eligibility(isk, 100, 10, 0) {
            Ok((eligible, _, _)) => {
                assert_eq!(eligible, Eligible::Yes);
            }
            Err(_) => assert!(false),
        }
        assert!(stakes.witnessing_eligibility_bool(isk, 100, 10, 0));
    }

    #[test]
    fn test_witnessing_eligibility_target_hash() {
        let mut stakes = <Stakes<String, _, _, _>>::with_minimum(100u64);
        let isk_1 = "validator_1";
        let isk_2 = "validator_2";
        let isk_3 = "validator_3";
        let isk_4 = "validator_4";

        stakes.add_stake(isk_1, 10_000_000_000, 0).unwrap();
        stakes.add_stake(isk_2, 20_000_000_000, 0).unwrap();
        stakes.add_stake(isk_3, 30_000_000_000, 0).unwrap();
        stakes.add_stake(isk_4, 40_000_000_000, 0).unwrap();

        match stakes.witnessing_eligibility(isk_1, 0, 2, 0) {
            // TODO: verify target hash
            Ok((eligible, _target_hash, _)) => {
                assert_eq!(
                    eligible,
                    Eligible::No(IneligibilityReason::InsufficientPower)
                );
            }
            Err(_) => assert!(false),
        }
        assert!(!stakes.witnessing_eligibility_bool(isk_1, 0, 10, 0));

        match stakes.witnessing_eligibility(isk_1, 100, 2, 0) {
            // TODO: verify target hash
            Ok((eligible, _target_hash, _)) => {
                assert_eq!(eligible, Eligible::No(IneligibilityReason::InsufficientPower));
            }
            Err(_) => assert!(false),
        }
        assert!(stakes.witnessing_eligibility_bool(isk_1, 100, 10, 0));
    }
}
