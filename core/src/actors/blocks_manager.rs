//! # Blocks Manager actor
//!
//! This module contains the Blocks Manager actor which is in charge
//! of managing the blocks of the Witnet blockchain received through
//! the protocol. Among its responsabilities lie the following:
//!
//! * Initializing the chain info upon running the node for the first time and persisting it into storage [StorageManager](actors::storage_manager::StorageManager)
//! * Recovering the chain info from storage and keeping it in its state.
//! * Validating block candidates as they come from a session
//! * Consolidating multiple block candidates for the same checkpoint into a single valid block.
//! * Putting valid blocks into storage by sending them to the storage manager actor.
//! * Having a method for letting other components to get blocks by *hash* or *checkpoint*.
//! * Having a method for letting other components to get the epoch of the current tip of the blockchain (e.g. last epoch field required for the handshake in the Witnet network protocol)

use actix;
use log::debug;

#[derive(Debug, Default)]
/// Blocks manager actor: manages the blocks of the Witnet blockchain
pub struct BlocksManager;

impl actix::Supervised for BlocksManager {}
impl actix::SystemService for BlocksManager {}

impl actix::Actor for BlocksManager {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        debug!("Blocks Manager actor has been started!");
    }
}
