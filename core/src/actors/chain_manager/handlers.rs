use actix::{
    ActorFuture, AsyncContext, Context, ContextFutureSpawner, Handler, System, WrapFuture,
};

use crate::actors::chain_manager::{messages::SessionUnitResult, ChainManager, ChainManagerError};
use crate::actors::epoch_manager::messages::EpochNotification;

use crate::actors::reputation_manager::{messages::ValidatePoE, ReputationManager};

use witnet_data_structures::{
    chain::{Block, CheckpointBeacon, InventoryEntry},
    error::{ChainInfoError, ChainInfoErrorKind, ChainInfoResult},
};

use crate::validations::{validate_coinbase, validate_merkle_tree};

use witnet_util::error::WitnetError;

use log::{debug, error, warn};

use super::messages::{
    AddNewBlock, BuildBlock, DiscardExistingInventoryEntries, GetBlock, GetBlocksEpochRange,
    GetHighestCheckpointBeacon, InventoryEntriesResult,
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

    fn handle(&mut self, msg: EpochNotification<EpochPayload>, _ctx: &mut Context<Self>) {
        debug!("Epoch notification received {:?}", msg.checkpoint);
    }
}

/// Handler for EpochNotification<EveryEpochPayload>
impl Handler<EpochNotification<EveryEpochPayload>> for ChainManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EveryEpochPayload>, _ctx: &mut Context<Self>) {
        debug!("Periodic epoch notification received {:?}", msg.checkpoint);
        self.current_epoch = Some(msg.checkpoint);
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
        // Block verify process
        let reputation_manager_addr = System::current().registry().get::<ReputationManager>();

        let block_epoch = msg.block.block_header.beacon.checkpoint;
        if self.current_epoch.is_none() {
            warn!("ChainManager doesn't have current epoch");
        } else if Some(block_epoch) != self.current_epoch {
            warn!("Block epoch not valid");
        } else if !validate_coinbase(&msg.block) {
            warn!("Block coinbase not valid");
        } else if !validate_merkle_tree(&msg.block) {
            warn!("Block merkle tree not valid");
        } else {
            // Request proof of eligibility validation to ReputationManager
            reputation_manager_addr
                .send(ValidatePoE {
                    beacon: msg.block.block_header.beacon,
                    proof: msg.block.proof,
                })
                .into_actor(self)
                .then(|res, act, _ctx| {
                    match res {
                        Err(e) => {
                            // Error when sending message
                            error!("Unsuccessful communication with reputation manager: {}", e);
                        }
                        Ok(false) => {
                            warn!("Block PoE not valid");
                        }
                        Ok(true) => {
                            // Insert in blocks mempool
                            let res = act.process_new_block(msg.block);
                            match res {
                                Ok(hash) => {
                                    act.broadcast_block(hash);
                                }
                                Err(ChainManagerError::BlockAlreadyExists) => {
                                    warn!("Block already exists");
                                }
                                Err(_) => {
                                    error!("Unexpected error");
                                }
                            };
                        }
                    }

                    actix::fut::ok(())
                })
                .wait(ctx);
        }
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
            .map(|epoch| &self.epoch_to_block_hash[&epoch])
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
