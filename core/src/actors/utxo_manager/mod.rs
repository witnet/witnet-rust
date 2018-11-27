//! # UTXO Manager
//!
//! The __UTXO manager__ is the actor that encapsulates the logic of the _unspent transaction outputs_, that is, it will be in charge of:
//!
//! * Keeping every unspent transaction output (UTXO) in the block chain in memory. This is called the _UTXO set_.
//! * Updating the UTXO set with valid transactions that have already been anchored into a valid block. This includes:
//!     - Removing the UTXOs that the transaction spends as inputs.
//!     - Adding a new UTXO for every output in the transaction.
mod actor;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// UtxoManager actor
#[derive(Default)]
pub struct UtxoManager;
