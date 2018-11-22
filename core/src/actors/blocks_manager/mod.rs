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
use actix::{
    ActorFuture, Context, ContextFutureSpawner, Supervised, System, SystemService, WrapFuture,
};

use witnet_data_structures::chain::ChainInfo;

use crate::actors::{
    storage_keys::CHAIN_KEY,
    storage_manager::{messages::Put, StorageManager},
};

use log::{error, info};

mod actor;
mod handlers;

/// Messages for BlocksManager
pub mod messages;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// BlocksManager actor
#[derive(Default)]
pub struct BlocksManager {
    /// Blockchain information data structure
    chain_info: Option<ChainInfo>,
}

/// Required trait for being able to retrieve BlocksManager address from registry
impl Supervised for BlocksManager {}

/// Required trait for being able to retrieve BlocksManager address from registry
impl SystemService for BlocksManager {}

/// Auxiliary methods for BlocksManager actor
impl BlocksManager {
    /// Method to persist chain_info into storage
    fn persist_chain_info(&self, ctx: &mut Context<Self>) {
        // Get StorageManager address
        let storage_manager_addr = System::current().registry().get::<StorageManager>();

        let chain_info = match self.chain_info.as_ref() {
            Some(x) => x,
            None => {
                error!("Trying to persist a None value");
                return;
            }
        };

        // Persist chain_info into storage. `AsyncContext::wait` registers
        // future within context, but context waits until this future resolves
        // before processing any other events.
        let msg = Put::from_value(CHAIN_KEY, chain_info).unwrap();
        storage_manager_addr
            .send(msg)
            .into_actor(self)
            .then(|res, _act, _ctx| {
                match res {
                    Ok(Ok(_)) => {
                        info!("BlocksManager successfully persisted chain_info into storage")
                    }
                    _ => {
                        error!("BlocksManager failed to persist chain_info into storage");
                        // FIXME(#72): handle errors
                    }
                }
                actix::fut::ok(())
            })
            .wait(ctx);
    }
}
