//! # Reputation Manager
//!
//! The __Reputation manager__ is the actor that encapsulates the
//! logic related to the reputation of the node inside the Witnet
//! network, that is, it will be in charge of:
//!
//! * Checking that proofs of eligibility are valid for the known
//! reputation of the issuers of such proofs
//! * Keeping score of the reputation balances for everyone in the
//! network
mod actor;

/// Message handlers for [`ReputationManager`](ReputationManager) actor
pub mod handlers;

/// Messages for [`ReputationManager`](ReputationManager) actor
pub mod messages;

/// Reputation Manager Actor
#[derive(Debug, Default)]
pub struct ReputationManager;
