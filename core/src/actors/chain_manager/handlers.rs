use actix::{Actor, AsyncContext, Context, Handler};

use crate::actors::chain_manager::{messages::SessionUnitResult, ChainManager, ChainManagerError};
use crate::actors::epoch_manager::messages::EpochNotification;

use witnet_data_structures::{
    chain::{Block, CheckpointBeacon, Hashable, Input, InventoryEntry, InventoryItem, Output},
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

    fn handle(&mut self, msg: AddTransaction, _ctx: &mut Context<Self>) {
        debug!("Transaction received");

        let outputs = &msg.transaction.outputs;
        let inputs = &msg.transaction.inputs;
        let outputs_from_inputs = &self.get_outputs_from_inputs(inputs);

        // Check that all inpunts point to unspend output
        if self.find_unspent_outputs(&msg.transaction.inputs) {
            // Validate transaction
            let inputs_sum = msg
                .transaction
                .calculate_outputs_sum_of(&outputs_from_inputs);
            let outputs_sum = msg.transaction.calculate_outputs_sum();
            let is_valid_transaction = outputs_sum < inputs_sum
                && validate_outputs(outputs, inputs)
                && validate_inputs(outputs_from_inputs, outputs);

            if is_valid_transaction {
                // Add valid transaction to transactions_pool
                self.transactions_pool
                    .insert(msg.transaction.hash(), msg.transaction);
            }
        }
    }
}

fn validate_inputs(outputs_from_inputs: &[Output], outputs: &[Output]) -> bool {
    let mut are_valid_inputs = true;
    let mut output_pointers_counter = 0;
    let outputs_from_inputs_len = outputs_from_inputs.len();

    while are_valid_inputs && output_pointers_counter < outputs_from_inputs_len {
        are_valid_inputs = validate_input(
            outputs_from_inputs[output_pointers_counter],
            outputs,
            output_pointers_counter,
        );
        output_pointers_counter += 1;
    }

    are_valid_inputs
}

fn validate_outputs(outputs: &[Output], inputs: &[Input]) -> bool {
    let mut are_valid_outputs = true;
    let mut outputs_counter = 0;
    let outputs_len = outputs.len();

    while are_valid_outputs && outputs_counter < outputs_len {
        are_valid_outputs = validate_output(outputs, outputs_counter, inputs);
        outputs_counter += 1;
    }

    are_valid_outputs
}

fn validate_output(outputs: &[Output], validation_index: usize, inputs: &[Input]) -> bool {
    match outputs[validation_index] {
        Output::ValueTransfer(_) => validate_vto_position(&outputs),
        Output::Consensus(_) => validate_to_uniqueness(&outputs),
        Output::DataRequest(_) => true,
        Output::Reveal(_) => {
            validate_ro_spending(outputs[validation_index])
                && validate_ro_coupled_position(inputs[validation_index])
                && validate_ro_tally_uniqueness(outputs)
        }
        Output::Commit(_) => validate_co_position(validation_index, inputs),
    }
}

fn validate_input(validating_output: Output, outputs: &[Output], validation_index: usize) -> bool {
    match validating_output {
        Output::ValueTransfer(_) => true,
        Output::Consensus(_) => true,
        Output::DataRequest(_) => match outputs[validation_index] {
            Output::Commit(_) => true,
            _ => false,
        },
        Output::Reveal(_) => match outputs[validation_index] {
            Output::ValueTransfer(_) => true,
            _ => false,
        },
        Output::Commit(_) => match outputs[validation_index] {
            Output::Reveal(_) | Output::Consensus(_) => true,
            _ => false,
        },
    }
}

fn validate_vto_position(outputs: &[Output]) -> bool {
    let mut counter = 0;
    let mut is_valid = true;
    let mut found = false;

    while is_valid && counter < outputs.len() {
        match outputs[counter] {
            Output::ValueTransfer(_) => {
                if !found {
                    found = true;
                }
            }
            _ => {
                if found {
                    is_valid = false;
                }
            }
        }
        counter += 1;
    }

    is_valid
}

fn validate_co_position(commit_output_index: usize, inputs: &[Input]) -> bool {
    match inputs[commit_output_index] {
        Input::DataRequest(_) => true,
        _ => false,
    }
}

fn validate_ro_spending(validating_output: Output) -> bool {
    match validating_output {
        Output::ValueTransfer(_) => true,
        _ => false,
    }
}

fn validate_ro_coupled_position(validating_input: Input) -> bool {
    match validating_input {
        Input::Commit(_) => true,
        _ => false,
    }
}

fn validate_ro_tally_uniqueness(outputs: &[Output]) -> bool {
    let mut counter = 0;
    let mut tally_output_counter = 0;
    let tally_output_max = 1;

    while tally_output_max <= tally_output_counter && counter < outputs.len() {
        match outputs[counter] {
            Output::Consensus(_) => {
                tally_output_counter += 1;
            }
            _ => continue,
        }
        counter += 1;
    }

    tally_output_counter == 1
}

fn validate_to_uniqueness(outputs: &[Output]) -> bool {
    let mut counter = 0;
    let mut tally_output_counter = 0;
    let tally_output_max = 1;

    while tally_output_max <= tally_output_counter && counter < outputs.len() {
        match outputs[counter] {
            Output::Consensus(_) => {
                tally_output_counter += 1;
            }
            _ => continue,
        }
        counter += 1;
    }

    tally_output_counter > tally_output_max
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
