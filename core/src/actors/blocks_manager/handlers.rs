use actix::{Context, Handler};

use crate::actors::blocks_manager::BlocksManager;
use crate::actors::epoch_manager::messages::EpochNotification;

use witnet_data_structures::{
    chain::Epoch,
    error::{ChainInfoError, ChainInfoErrorKind, ChainInfoResult},
};

use witnet_util::error::WitnetError;

use log::{debug, error};

use super::messages::GetHighestBlockCheckpoint;

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
impl Handler<EpochNotification<EpochPayload>> for BlocksManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EpochPayload>, _ctx: &mut Context<Self>) {
        debug!("Epoch notification received {:?}", msg.checkpoint);
    }
}

/// Handler for EpochNotification<EveryEpochPayload>
impl Handler<EpochNotification<EveryEpochPayload>> for BlocksManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EveryEpochPayload>, _ctx: &mut Context<Self>) {
        debug!("Periodic epoch notification received {:?}", msg.checkpoint);
    }
}

/// Handler for GetHighestBlockCheckpoint message
impl Handler<GetHighestBlockCheckpoint> for BlocksManager {
    type Result = ChainInfoResult<Epoch>;

    fn handle(
        &mut self,
        _msg: GetHighestBlockCheckpoint,
        _ctx: &mut Context<Self>,
    ) -> Self::Result {
        if let Some(chain_info) = &self.chain_info {
            Ok(chain_info.highest_block_checkpoint.checkpoint)
        } else {
            error!("No ChainInfo loaded in BlocksManager");
            Err(WitnetError::from(ChainInfoError::new(
                ChainInfoErrorKind::ChainInfoNotFound,
                "No ChainInfo loaded in BlocksManager".to_string(),
            )))
        }
    }
}
