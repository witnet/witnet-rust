use std::{fmt, ops};

pub use num_traits::Zero;
use serde::{Deserialize, Serialize};

use crate::{chain::priority::Priority, wit::Wit};

#[derive(Copy, Clone, Debug, Deserialize, Hash, PartialEq, Eq, PartialOrd, Serialize)]
pub struct AbsoluteFee(Wit);

impl AbsoluteFee {
    #[inline]
    pub fn as_nanowits(&self) -> u64 {
        self.0.nanowits()
    }

    #[inline]
    pub fn into_inner(self) -> Wit {
        self.0
    }
}

impl fmt::Display for AbsoluteFee {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ops::Add for AbsoluteFee {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Zero for AbsoluteFee {
    #[inline]
    fn zero() -> Self {
        Self(Wit::zero())
    }

    #[inline]
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Hash, PartialEq, Eq, PartialOrd, Serialize)]
pub struct RelativeFee(Priority);

impl RelativeFee {
    #[inline]
    pub fn into_absolute(self, weight: u32) -> AbsoluteFee {
        AbsoluteFee(self.0.derive_fee_wit(weight))
    }
}

impl fmt::Display for RelativeFee {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} nWitWu", self.0.as_f64())
    }
}

impl ops::Add for RelativeFee {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl num_traits::Zero for RelativeFee {
    #[inline]
    fn zero() -> Self {
        Self(Priority::zero())
    }

    #[inline]
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Hash, PartialEq, Eq, PartialOrd, Serialize)]
/// Type for representing a fee value that can be absolute (nanoWits) or relative (priority).
pub enum Fee {
    /// An absolute fee, as expressed in nanoWits.
    Absolute(AbsoluteFee),
    /// A relative fee, aka "priority", as expressed as nanoWits (or fractional amounts) per weightunit.
    Relative(RelativeFee),
}

impl Fee {
    #[inline]
    pub fn absolute_from_wit(wit: Wit) -> Self {
        Self::Absolute(AbsoluteFee(wit))
    }

    #[inline]
    pub fn absolute_from_nanowits(nanowits: u64) -> Self {
        Self::absolute_from_wit(Wit::from_nanowits(nanowits))
    }

    pub fn relative_from_float<T>(float: T) -> Self
    where
        f64: From<T>,
    {
        Self::Relative(RelativeFee(Priority::from(f64::from(float))))
    }
}

impl Default for Fee {
    fn default() -> Self {
        <Self as Zero>::zero()
    }
}

impl fmt::Display for Fee {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fee::Absolute(absolute) => absolute.fmt(f),
            Fee::Relative(relative) => relative.fmt(f),
        }
    }
}

impl ops::Add for Fee {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        use crate::fee::Fee::*;
        match (self, rhs) {
            (Absolute(lhs), Absolute(rhs)) => Fee::Absolute(lhs + rhs),
            (Relative(lhs), Relative(rhs)) => Fee::Relative(lhs + rhs),
            _ => {
                unimplemented!()
            }
        }
    }
}

impl num_traits::Zero for Fee {
    #[inline]
    fn zero() -> Self {
        Self::Absolute(AbsoluteFee::zero())
    }

    fn is_zero(&self) -> bool {
        match self {
            Fee::Absolute(absolute) => absolute.is_zero(),
            Fee::Relative(relative) => relative.is_zero(),
        }
    }
}
