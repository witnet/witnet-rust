use futures::Future;
use log::*;
use web3::types::{TransactionReceipt, U256};
use witnet_data_structures::chain::Block;

pub mod block_ticker;
pub mod eth_event_stream;
pub mod main_actor;
pub mod post_actor;
pub mod report_ticker;
pub mod wbi_requests_initial_sync;
pub mod witnet_block_stream;

/// Message to the post actor, which will try to claim data requests from the
/// WBI and post them on Witnet
#[derive(Debug)]
pub enum PostActorMessage {
    /// A new data request was just posted, try to claim it
    NewDr(U256),
    /// Periodic tick to check if old data requests can be claimed again
    Tick,
}

/// Message to the main actor
#[derive(Debug)]
pub enum ActorMessage {
    /// The subscription to new Witnet blocks just sent us a new block
    NewWitnetBlock(Box<Block>),
    /// This old block may have tallies for data requests whose inclusion can
    /// be reported to the WBI
    ReplayWitnetBlock(Box<Block>),
}

/// Handle Ethereum transaction receipt
pub fn handle_receipt(receipt: TransactionReceipt) -> impl Future<Item = (), Error = ()> {
    match receipt.status {
        Some(x) if x == 1.into() => {
            //debug!("Success!");
            // Success
            futures::finished(())
        }
        Some(x) if x == 0.into() => {
            error!("Error :(");
            // Fail
            // TODO: Reason?
            futures::failed(())
        }
        x => {
            error!("Wtf is a {:?} status", x);
            futures::failed(())
        }
    }
}
