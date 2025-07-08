use std::{fmt, iter::Sum, ops::*};

use serde::{Deserialize, Serialize};

use crate::{chain::Epoch, staking::helpers::Power};

/// 1 nanowit is the minimal unit of value
/// 1 wit = 10^9 nanowits
pub const NANOWITS_PER_WIT: u64 = 1_000_000_000;
// 10 ^ WIT_DECIMAL_PLACES
/// Number of decimal places used in the string representation of wit value.
pub const WIT_DECIMAL_PLACES: u8 = 9;

/// Unit of value
#[derive(
    Clone, Copy, Debug, Deserialize, Default, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize,
)]
pub struct Wit(u64);

impl Wit {
    /// Create from wits
    #[inline]
    pub fn from_wits(wits: u64) -> Self {
        Self::from_nanowits(wits.checked_mul(NANOWITS_PER_WIT).expect("overflow"))
    }

    /// Create from nanowits
    #[inline]
    pub fn from_nanowits(nanowits: u64) -> Self {
        Self(nanowits)
    }

    /// Retrieve the nanowits value within.
    #[inline]
    pub fn nanowits(self) -> u64 {
        self.0
    }

    /// Return integer and fractional part, useful for pretty printing
    pub fn wits_and_nanowits(self) -> (u64, u64) {
        let nanowits = self.0;
        let amount_wits = nanowits / NANOWITS_PER_WIT;
        let amount_nanowits = nanowits % NANOWITS_PER_WIT;

        (amount_wits, amount_nanowits)
    }
}

impl fmt::Display for Wit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (amount_wits, amount_nanowits) = self.wits_and_nanowits();
        let width = usize::from(WIT_DECIMAL_PLACES);

        write!(f, "{amount_wits}.{amount_nanowits:0width$}",)
    }
}

impl Add for Wit {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self::from_nanowits(self.nanowits() + rhs.nanowits())
    }
}

impl Div for Wit {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self::from_nanowits(self.nanowits() / rhs.nanowits())
    }
}

impl Rem for Wit {
    type Output = Self;

    fn rem(self, rhs: Self) -> Self::Output {
        Self::from_nanowits(self.nanowits() % rhs.nanowits())
    }
}

impl Mul for Wit {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        Self::from_nanowits(self.nanowits() * rhs.nanowits())
    }
}

impl Mul<Epoch> for Wit {
    type Output = Power;

    fn mul(self, rhs: Epoch) -> Self::Output {
        Power::from(self.nanowits() * u64::from(rhs))
    }
}

impl Sub for Wit {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self::from_nanowits(self.nanowits() - rhs.nanowits())
    }
}

impl num_traits::Zero for Wit {
    #[inline]
    fn zero() -> Self {
        Wit(0)
    }

    #[inline]
    fn is_zero(&self) -> bool {
        matches!(self, &Wit(0))
    }
}

impl num_traits::ops::saturating::Saturating for Wit {
    fn saturating_add(self, v: Self) -> Self {
        Self::from_nanowits(self.nanowits().saturating_add(v.nanowits()))
    }

    fn saturating_sub(self, v: Self) -> Self {
        Self::from_nanowits(self.nanowits().saturating_sub(v.nanowits()))
    }
}

impl From<u64> for Wit {
    fn from(value: u64) -> Self {
        Self::from_nanowits(value)
    }
}

impl From<Wit> for u64 {
    fn from(value: Wit) -> Self {
        value.0
    }
}

impl Sum for Wit {
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = Wit>,
    {
        let mut total = Wit::from_nanowits(0);
        for w in iter {
            total = total + w;
        }
        total
    }
}

/// Trait defining numeric data types that provide methods for changing their decimal dot position.
///
/// That is, a precision loss of 3 digits applied on number 10_000 will give 10.
///
/// This allows a type to increase its range at the cost of precision.
pub trait PrecisionLoss: Copy {
    fn lose_precision(self, digits: u8) -> Self;
}

impl PrecisionLoss for u64 {
    fn lose_precision(self, digits: u8) -> u64 {
        self / 10_u64.pow(u32::from(digits))
    }
}

impl PrecisionLoss for Wit {
    fn lose_precision(self, digits: u8) -> Wit {
        Wit(self.0.lose_precision(digits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wit_decimal_places() {
        // 10 ^ WIT_DECIMAL_PLACES == NANOWITS_PER_WIT
        assert_eq!(10u64.pow(u32::from(WIT_DECIMAL_PLACES)), NANOWITS_PER_WIT);
    }

    #[test]
    fn wit_pretty_print() {
        assert_eq!(Wit::from_nanowits(0).to_string(), "0.000000000");
        assert_eq!(Wit::from_nanowits(1).to_string(), "0.000000001");
        assert_eq!(Wit::from_nanowits(90).to_string(), "0.000000090");
        assert_eq!(Wit::from_nanowits(890).to_string(), "0.000000890");
        assert_eq!(Wit::from_nanowits(7_890).to_string(), "0.000007890");
        assert_eq!(Wit::from_nanowits(67_890).to_string(), "0.000067890");
        assert_eq!(Wit::from_nanowits(567_890).to_string(), "0.000567890");
        assert_eq!(Wit::from_nanowits(4_567_890).to_string(), "0.004567890");
        assert_eq!(Wit::from_nanowits(34_567_890).to_string(), "0.034567890");
        assert_eq!(Wit::from_nanowits(234_567_890).to_string(), "0.234567890");
        assert_eq!(Wit::from_nanowits(1_234_567_890).to_string(), "1.234567890");
        assert_eq!(
            Wit::from_nanowits(21_234_567_890).to_string(),
            "21.234567890"
        );
        assert_eq!(
            Wit::from_nanowits(321_234_567_890).to_string(),
            "321.234567890"
        );
    }
}
