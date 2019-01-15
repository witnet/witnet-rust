//! # RAD Engine

pub mod error;
pub mod operators;
pub mod script;
pub mod types;

/// Run retrieval stage of a data request.
pub fn run_retrieval() {}

/// Run aggregate stage of a data request.
pub fn run_aggregation() {}

/// Run consensus stage of a data request.
pub fn run_consensus() {}

/// Run deliver clauses of a data request.
pub fn run_delivery() {}
