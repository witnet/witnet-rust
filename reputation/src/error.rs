//! Error type definitions for the reputation module.

use failure::Fail;
use std::fmt;

/// The error type for operations in Reputation module
#[derive(Debug, PartialEq, Eq, Fail)]
pub enum ReputationError {
    /// Proposed time for updating is previous to current
    #[fail(
        display = "Proposed time for updating ({}) is previous to current ({})",
        new_time, current_time
    )]
    InvalidUpdateTime { new_time: u32, current_time: u32 },
}

/// Received an alpha < max_alpha
#[derive(Debug, PartialEq, Eq)]
pub struct NonSortedAlpha<A> {
    pub alpha: A,
    pub max_alpha: A,
}

impl<A> fmt::Display for NonSortedAlpha<A>
where
    A: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Received an alpha < max_alpha: {:?} < {:?}",
            self.alpha, self.max_alpha
        )
    }
}

impl<A> Fail for NonSortedAlpha<A> where A: 'static + fmt::Debug + Send + Sync {}

/// Error in the penalization function
#[derive(Debug, PartialEq, Eq)]
pub struct RepError<V> {
    pub old_rep: V,
    pub new_rep: V,
}

impl<V> fmt::Display for RepError<V>
where
    V: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Penalization function returned more reputation than allowed: {:?} > {:?}",
            self.new_rep, self.old_rep
        )
    }
}

impl<V> Fail for RepError<V> where V: 'static + fmt::Debug + Send + Sync {}
