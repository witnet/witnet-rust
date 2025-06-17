//! Error type definitions for the reputation module.

use std::fmt;
use thiserror::Error;

/// The error type for operations in Reputation module
#[derive(Debug, PartialEq, Eq, Error)]
pub enum ReputationError {
    /// Proposed time for updating is previous to current
    #[error("Proposed time for updating ({new_time}) is previous to current ({current_time})")]
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
