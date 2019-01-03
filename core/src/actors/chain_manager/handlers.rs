use actix::{Actor, AsyncContext, Context, Handler};

use crate::actors::chain_manager::{messages::SessionUnitResult, ChainManager, ChainManagerError};
use crate::actors::epoch_manager::messages::EpochNotification;

use witnet_data_structures::{
    chain::{Block, CheckpointBeacon, Hashable, InventoryEntry, InventoryItem},
    error::{ChainInfoError, ChainInfoErrorKind, ChainInfoResult},
};

use witnet_util::error::WitnetError;

use log::{debug, error, warn};

use super::messages::{
    AddNewBlock, AddTransaction, BuildBlock, DiscardExistingInventoryEntries, GetBlock,
    GetBlocksEpochRange, GetHighestCheckpointBeacon, InventoryEntriesResult,
};

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////
/// Payload for the notification for a specific epoch
#[derive(Debug)]
pub struct EpochPayload;

/// Payload for the notification for all epochs
#[derive(Clone, Debug)]
pub struct EveryEpochPayload;

/// Handler for EpochNotification<EpochPayload>
impl Handler<EpochNotification<EpochPayload>> for ChainManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EpochPayload>, ctx: &mut Context<Self>) {
        debug!("Epoch notification received {:?}", msg.checkpoint);

        // Genesis checkpoint notification. We need to start building the chain.
        if msg.checkpoint == 0 {
            warn!("Genesis checkpoint is here! Starting to bootstrap the chain...");
            self.started(ctx);
        }
    }
}

/// Handler for EpochNotification<EveryEpochPayload>
impl Handler<EpochNotification<EveryEpochPayload>> for ChainManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EveryEpochPayload>, ctx: &mut Context<Self>) {
        debug!("Periodic epoch notification received {:?}", msg.checkpoint);
        self.current_epoch = Some(msg.checkpoint);

        if let Some(candidate) = self.block_candidate.take() {
            // Update chain_info
            match self.chain_info.as_mut() {
                Some(chain_info) => {
                    let beacon = CheckpointBeacon {
                        checkpoint: msg.checkpoint,
                        hash_prev_block: candidate.hash(),
                    };

                    chain_info.highest_block_checkpoint = beacon;
                }
                None => {
                    error!("No ChainInfo loaded in ChainManager");
                }
            }

            // Send block to Inventory Manager
            self.persist_item(ctx, InventoryItem::Block(candidate));

            // Persist chain_info into storage
            self.persist_chain_info(ctx);
        }
    }
}

/// Handler for GetHighestBlockCheckpoint message
impl Handler<GetHighestCheckpointBeacon> for ChainManager {
    type Result = ChainInfoResult<CheckpointBeacon>;

    fn handle(
        &mut self,
        _msg: GetHighestCheckpointBeacon,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        if let Some(chain_info) = &self.chain_info {
            Ok(chain_info.highest_block_checkpoint)
        } else {
            error!("No ChainInfo loaded in ChainManager");
            Err(WitnetError::from(ChainInfoError::new(
                ChainInfoErrorKind::ChainInfoNotFound,
                "No ChainInfo loaded in ChainManager".to_string(),
            )))
        }
    }
}

/// Handler for AddNewBlock message
impl Handler<AddNewBlock> for ChainManager {
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: AddNewBlock, ctx: &mut Context<Self>) {
        self.process_block(ctx, msg.block)
    }
}

/// Handler for AddTransaction message
impl Handler<AddTransaction> for ChainManager {
    type Result = SessionUnitResult;

    fn handle(&mut self, _msg: AddTransaction, _ctx: &mut Context<Self>) {
        // FIXME(#240) Implement transaction process
        debug!("Transaction received");
    }
}

/// Handler for GetBlock message
impl Handler<GetBlock> for ChainManager {
    type Result = Result<Block, ChainManagerError>;

    fn handle(
        &mut self,
        msg: GetBlock,
        _ctx: &mut Context<Self>,
    ) -> Result<Block, ChainManagerError> {
        // Try to get block by hash
        self.try_to_get_block(msg.hash)
    }
}

/// Handler for GetBlocksEpochRange
impl Handler<GetBlocksEpochRange> for ChainManager {
    type Result = Result<Vec<InventoryEntry>, ChainManagerError>;

    fn handle(
        &mut self,
        GetBlocksEpochRange { range }: GetBlocksEpochRange,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        debug!("GetBlocksEpochRange received {:?}", range);
        let hashes = range
            .flat_map(|epoch| self.epoch_to_block_hash.get(&epoch))
            .flatten()
            .map(|hash| InventoryEntry::Block(*hash))
            .collect();

        Ok(hashes)
    }
}

/// Handler for BuildBlock message
impl Handler<BuildBlock> for ChainManager {
    type Result = ();

    fn handle(&mut self, msg: BuildBlock, ctx: &mut Context<Self>) -> Self::Result {
        // Build the block using the supplied beacon and eligibility proof
        let block = self.build_block(&msg);

        // Send AddNewBlock message to self
        // This will run all the validations again
        ctx.notify(AddNewBlock { block })
    }
}

/// Handler for DiscardExistingInvVectors message
impl Handler<DiscardExistingInventoryEntries> for ChainManager {
    type Result = InventoryEntriesResult;

    fn handle(
        &mut self,
        msg: DiscardExistingInventoryEntries,
        _ctx: &mut Context<Self>,
    ) -> InventoryEntriesResult {
        // Discard existing inventory vectors
        self.discard_existing_inventory_entries(msg.inv_entries)
    }
}
