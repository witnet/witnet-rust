use std::{fmt, ops, str};

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

impl str::FromStr for AbsoluteFee {
    type Err = <u64 as str::FromStr>::Err;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str(s).map(Wit::from_nanowits).map(Self)
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

impl str::FromStr for RelativeFee {
    type Err = <f64 as str::FromStr>::Err;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        f64::from_str(s).map(Priority::from).map(Self)
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
        Self::absolute_from_nanowits(0)
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

/// Allow backwards compatibility with old Wallet API clients that may provide fee values without
/// tagging whether they are absolute or relative.
///
/// This implicitly treats integers as absolute fees and floats as relative fees. Strings encoding
/// numbers are also parsed in the same way.
pub fn deserialize_fee_backwards_compatible<'de, D>(deserializer: D) -> Result<Fee, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Untagged {
        Fee(Fee),
        Integer(u64),
        String(String),
    }

    Ok(match Untagged::deserialize(deserializer)? {
        Untagged::Fee(fee) => fee,
        Untagged::Integer(integer) => Fee::absolute_from_nanowits(integer),
        Untagged::String(string) => string
            .parse::<u64>()
            .map(Fee::absolute_from_nanowits)
            .map_err(serde::de::Error::custom)?,
    })
}
