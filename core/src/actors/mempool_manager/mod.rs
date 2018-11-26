//! # MempoolManager actor
//!
//! This module contains the MempoolManager actor which is in charge
//! of managing and validating the transactions received through
//! the protocol. Among its responsabilities are the following:
//!
//! * Validating transactions as they come from any [Session](actors::session::Session). This includes:
//!     - Iterating over its inputs, asking the [UtxoManager](actors::utxo_manager::UtxoManager) for the to-be-spent UTXOs and adding the value of the inputs to calculate the value of the transaction.
//!     - Running the output scripts, expecting them all to return `TRUE` and leave an empty stack.
//!     - Verifying that the sum of all inputs is greater than or equal to the sum of all the outputs.
//! * Keeping valid transactions into memory. This in-memory transaction pool is what we call the _mempool_. Valid transactions are immediately appended to the mempool.
//! * Receiving confirmation notifications from [BlocksManager](actors::blocks_manager::BlocksManager). This notifications tell that a certain transaction ID has been anchored into a new block and thus it can be removed from the mempool and persisted into local storage (for archival purposes, non-archival nodes can just drop them).
//! * Notifying [UtxoManager](actors::utxo_manager::UtxoManager) for it to apply a valid transaction on the UTXO set.

mod actor;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// MempoolManager actor
#[derive(Default)]
pub struct MempoolManager;
