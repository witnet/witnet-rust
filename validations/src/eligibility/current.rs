use std::{
    fmt::{Debug, Display},
    iter::Sum,
    ops::{Add, AddAssign, Div, Mul, Rem, Sub},
};

use serde::Serialize;

use witnet_crypto::secp256k1::serde;
use witnet_data_structures::{chain::Hash, staking::prelude::*, wit::PrecisionLoss};

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
pub trait Eligibility<Address, Coins, Epoch, Nonce, Power>
where
    Address: Debug + Display + Sync + Send + 'static,
    Coins: Debug + Display + Sync + Send + Sum + 'static,
    Epoch: Debug + Display + Sync + Send + 'static,
    Nonce: Debug + Display + Sync + Send + 'static,
{
    /// Tells whether a VRF proof meets the requirements to become eligible for mining. Unless an error occurs, returns
    /// an `Eligibility` structure signaling eligibility or lack thereof (in which case you also get an
    /// `IneligibilityReason`.
    fn mining_eligibility<ISK>(
        &self,
        validator: ISK,
        epoch: Epoch,
        rf: u16,
    ) -> StakesResult<Eligible, Address, Coins, Epoch>
    where
        ISK: Into<Address>;

    /// Tells whether a VRF proof meets the requirements to become eligible for mining. Because this function returns a
    /// simple `bool`, it is best-effort: both lack of eligibility and any error cases are mapped to `false`.
    fn mining_eligibility_bool<ISK>(&self, validator: ISK, epoch: Epoch, rf: u16) -> bool
    where
        ISK: Into<Address>,
    {
        matches!(
            self.mining_eligibility(validator, epoch, rf),
            Ok(Eligible::Yes)
        )
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
        max_rounds: u16,
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
        max_rounds: u16,
    ) -> bool
    where
        ISK: Into<Address>,
    {
        match self.witnessing_eligibility(validator, epoch, witnesses, round, max_rounds) {
            Ok((eligible, _, _)) => matches!(eligible, Eligible::Yes),
            Err(_) => false,
        }
    }
}

impl<const UNIT: u8, Address, Coins, Epoch, Nonce, Power>
    Eligibility<Address, Coins, Epoch, Nonce, Power>
    for Stakes<UNIT, Address, Coins, Epoch, Nonce, Power>
where
    Address: Clone + Debug + Default + Display + Ord + Sync + Send + Serialize + 'static,
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
        + Serialize
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
        + Serialize
        + PartialOrd
        + 'static,
    Nonce: Copy
        + Debug
        + Default
        + Display
        + num_traits::Saturating
        + AddAssign
        + From<u32>
        + Sync
        + Send
        + Serialize
        + 'static,
    Power: Copy
        + Default
        + Ord
        + Add<Output = Power>
        + Sub<Output = Power>
        + Mul<Output = Power>
        + Div<Output = Power>
        + From<u64>
        + Serialize
        + Sum
        + Display,
    u64: From<Coins> + From<Power> + Mul<Power, Output = u64> + Div<Power, Output = u64>,
{
    fn mining_eligibility<ISK>(
        &self,
        validator: ISK,
        epoch: Epoch,
        replication_factor: u16,
    ) -> StakesResult<Eligible, Address, Coins, Epoch>
    where
        ISK: Into<Address>,
    {
        let validator: Address = validator.into();

        // Cap replication factor to 2/3rds of total stake entries count
        let max_replication_factor = u16::try_from((((self.stakes_count() * 2) as f64) / 3.0) as u32).unwrap_or(u16::MAX);
        let replication_factor = if replication_factor > max_replication_factor {
            max_replication_factor
        } else {
            replication_factor
        };

        Ok(
            match self.by_rank(Capability::Mining, epoch)
                .take(replication_factor as usize)
                .find(|(key, _)| key.validator == validator)
            {
                Some(_) => Eligible::Yes,
                None => IneligibilityReason::InsufficientPower.into()
            }
        )
    }

    fn witnessing_eligibility<ISK>(
        &self,
        key: ISK,
        epoch: Epoch,
        witnesses: u16,
        round: u16,
        max_rounds: u16,
    ) -> StakesResult<(Eligible, Hash, f64), Address, Coins, Epoch>
    where
        ISK: Into<Address>,
    {
        let power = match self.query_power(key, Capability::Witnessing, epoch) {
            Ok(p) => p,
            Err(e) => {
                // Early exit if the stake key does not exist
                return match e {
                    StakesError::ValidatorNotFound { .. } => {
                        Ok((IneligibilityReason::NotStaking.into(), Hash::min(), 0.0))
                    }
                    e => Err(e),
                };
            }
        };

        // Validators with power 0 should not be eligible to mine a block
        if power == Power::from(0) {
            return Ok((
                IneligibilityReason::InsufficientPower.into(),
                Hash::min(),
                0.0,
            ));
        }

        let mut rank = self.by_rank(Capability::Witnessing, epoch);
        let (_, max_power) = rank.next().unwrap_or_default();

        // Requirement no. 2 from the WIP:
        //  "the witnessing power of the block proposer is in the `rf / stakers`th quantile among the witnessing powers
        //  of all the stakers"
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
        //  `max_rounds * own_power / (round * threshold_power + max_power * (max_rounds - round))`"
        let dividend = Power::from(u64::from(max_rounds))
            * Power::from((u64::BITS - u64::from(power).leading_zeros()).into());
        let divisor = u32::from(round)
            .saturating_mul(u64::BITS - u64::from(threshold_power).leading_zeros())
            .saturating_add(
                (u64::BITS - u64::from(max_power).leading_zeros())
                    .saturating_mul((max_rounds - round).into()),
            );
        let (target_hash, probability) = if divisor == 0 {
            (Hash::with_first_u32(u32::MAX), 1_f64)
        } else {
            let hash = Hash::with_first_u32(
                (((u64::MAX / Power::from(u64::from(divisor)))
                    .saturating_mul(u64::from(dividend)))
                    >> 32)
                    .try_into()
                    .unwrap(),
            );

            #[allow(clippy::cast_precision_loss, clippy::cast_lossless)]
            let probability: f64 = u64::from(dividend) as f64 / divisor as f64;

            (hash, probability)
        };

        Ok((Eligible::Yes, target_hash, probability))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_STAKE_NANOWITS: u64 = 10_000_000_000;

    #[test]
    fn test_mining_eligibility_no_stakers() {
        let stakes = StakesTester::default();
        let isk = "validator";

        assert_eq!(
            stakes.mining_eligibility(isk, 0, 4),
            Ok(Eligible::No(IneligibilityReason::NotStaking))
        );
        assert!(!stakes.mining_eligibility_bool(isk, 0, 4));

        assert_eq!(
            stakes.mining_eligibility(isk, 100, 4),
            Ok(Eligible::No(IneligibilityReason::NotStaking))
        );
        assert!(!stakes.mining_eligibility_bool(isk, 100, 4));
    }

    #[test]
    fn test_mining_eligibility_absolute_power() {
        let mut stakes = StakesTester::default();
        let isk = "validator";

        stakes
            .add_stake(isk, 10_000_000_000, 0, MIN_STAKE_NANOWITS)
            .unwrap();

        assert_eq!(
            stakes.mining_eligibility(isk, 0, 4),
            Ok(Eligible::No(IneligibilityReason::InsufficientPower))
        );
        assert!(!stakes.mining_eligibility_bool(isk, 0, 4));

        assert_eq!(stakes.mining_eligibility(isk, 100, 4), Ok(Eligible::Yes));
        assert!(stakes.mining_eligibility_bool(isk, 100, 4));
    }

    #[test]
    fn test_witnessing_eligibility_no_stakers() {
        let stakes = StakesTester::default();
        let isk = "validator";

        let eligibility = stakes.witnessing_eligibility(isk, 0, 10, 0, 4);
        assert!(matches!(
            eligibility,
            Ok((Eligible::No(IneligibilityReason::NotStaking), _, _))
        ));
        assert!(!stakes.witnessing_eligibility_bool(isk, 0, 10, 0, 4));

        let eligibility = stakes.witnessing_eligibility(isk, 100, 10, 0, 4);
        assert!(matches!(
            eligibility,
            Ok((Eligible::No(IneligibilityReason::NotStaking), _, _))
        ));
        assert!(!stakes.witnessing_eligibility_bool(isk, 100, 10, 0, 4));
    }

    #[test]
    fn test_witnessing_eligibility_absolute_power() {
        let mut stakes = StakesTester::default();
        let isk = "validator";

        stakes
            .add_stake(isk, 10_000_000_000, 0, MIN_STAKE_NANOWITS)
            .unwrap();

        let eligibility = stakes.witnessing_eligibility(isk, 0, 10, 0, 4);
        assert!(matches!(
            eligibility,
            Ok((Eligible::No(IneligibilityReason::InsufficientPower), _, _))
        ));
        assert!(!stakes.witnessing_eligibility_bool(isk, 0, 10, 0, 4));

        let eligibility = stakes.witnessing_eligibility(isk, 100, 10, 0, 4);
        assert!(matches!(eligibility, Ok((Eligible::Yes, _, _))));
        assert!(stakes.witnessing_eligibility_bool(isk, 100, 10, 0, 4));
    }

    #[test]
    fn test_witnessing_eligibility_target_hash() {
        let mut stakes = StakesTester::default();
        let isk_1 = "validator_1";
        let isk_2 = "validator_2";
        let isk_3 = "validator_3";
        let isk_4 = "validator_4";

        stakes
            .add_stake(isk_1, 10_000_000_000, 0, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(isk_2, 20_000_000_000, 0, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(isk_3, 30_000_000_000, 0, MIN_STAKE_NANOWITS)
            .unwrap();
        stakes
            .add_stake(isk_4, 40_000_000_000, 0, MIN_STAKE_NANOWITS)
            .unwrap();

        let eligibility = stakes.witnessing_eligibility(isk_1, 0, 2, 0, 4);
        // TODO: verify target hash
        assert!(matches!(
            eligibility,
            Ok((Eligible::No(IneligibilityReason::InsufficientPower), _, _))
        ));
        assert!(!stakes.witnessing_eligibility_bool(isk_1, 0, 10, 0, 4));

        let eligibility = stakes.witnessing_eligibility(isk_1, 100, 2, 0, 4);
        // TODO: verify target hash
        assert!(matches!(
            eligibility,
            Ok((Eligible::No(IneligibilityReason::InsufficientPower), _, _))
        ));
        assert!(!stakes.witnessing_eligibility_bool(isk_1, 0, 10, 0, 4));
        assert!(stakes.witnessing_eligibility_bool(isk_1, 100, 10, 0, 4));
    }
}
