use futures::Future;
use log::*;
use web3::types::{TransactionReceipt, U256};
use witnet_data_structures::chain::Block;

pub mod block_relay_and_poi;
pub mod block_relay_check;
pub mod claim_and_post;
pub mod eth_event_stream;
pub mod tally_finder;
pub mod wbi_requests_initial_sync;
pub mod witnet_block_stream;

/// Message to the claim_and_post actor, which will try to claim data requests from the
/// WBI and post them on Witnet on success
#[derive(Debug)]
pub enum ClaimMsg {
    /// A new data request was just posted, try to claim it
    NewDr(U256),
    /// Periodic tick to check if old data requests can be claimed again
    Tick,
}

/// Message to the block_relay_and_poi actor
#[derive(Debug)]
pub enum WitnetBlock {
    /// The subscription to new Witnet blocks just sent us a new block.
    /// Post it to the block relay, and process data requests and tallies.
    New(Block),
    /// This old block may have tallies for data requests whose inclusion can
    /// be reported to the WBI.
    /// Process data requests and tallies.
    Replay(Block),
}

/// Handle Ethereum transaction receipt
// This function returns a future because in the future it may be possible
// to retrieve the failure reason (for example: transaction reverted, invalid
// opcode).
pub fn handle_receipt(receipt: TransactionReceipt) -> impl Future<Item = (), Error = ()> {
    match receipt.status {
        Some(x) if x == 1.into() => {
            // Success
            futures::finished(())
        }
        Some(x) if x == 0.into() => {
            // Fail
            futures::failed(())
        }
        x => {
            error!("Unknown return code, should be 0 or 1, is: {:?}", x);
            futures::failed(())
        }
    }
}
