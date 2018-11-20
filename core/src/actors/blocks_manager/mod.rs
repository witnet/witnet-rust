//! # BlocksManager actor
//!
//! This module contains the BlocksManager actor which is in charge
//! of managing the blocks of the Witnet blockchain received through
//! the protocol. Among its responsabilities are the following:
//!
//! * Initializing the chain info upon running the node for the first time and persisting it into storage [StorageManager](actors::storage_manager::StorageManager)
//! * Recovering the chain info from storage and keeping it in its state.
//! * Validating block candidates as they come from a session.
//! * Consolidating multiple block candidates for the same checkpoint into a single valid block.
//! * Putting valid blocks into storage by sending them to the storage manager actor.
//! * Having a method for letting other components get blocks by *hash* or *checkpoint*.
//! * Having a method for letting other components get the epoch of the current tip of the
//! blockchain (e.g. the last epoch field required for the handshake in the Witnet network
//! protocol).

use actix::{Supervised, SystemService};

mod actor;
mod handlers;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// BlocksManager actor
#[derive(Default)]
pub struct BlocksManager {}

/// Required trait for being able to retrieve BlocksManager address from registry
impl Supervised for BlocksManager {}

/// Required trait for being able to retrieve BlocksManager address from registry
impl SystemService for BlocksManager {}
