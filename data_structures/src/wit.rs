use std::fmt;

use serde::{Deserialize, Serialize};

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
        Self(wits.checked_mul(NANOWITS_PER_WIT).expect("overflow"))
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

        write!(
            f,
            "{}.{:0width$}",
            amount_wits,
            amount_nanowits,
            width = width
        )
    }
}

impl std::ops::Add for Wit {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.nanowits() + rhs.nanowits())
    }
}

impl std::ops::Sub for Wit {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.nanowits() - rhs.nanowits())
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
