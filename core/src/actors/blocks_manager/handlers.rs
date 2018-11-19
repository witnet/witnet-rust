use actix::{Context, Handler};

use crate::actors::blocks_manager::BlocksManager;
use crate::actors::epoch_manager::messages::EpochNotification;

use log::debug;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////
/// Payload for the notification for a specific epoch
pub struct EpochMessage;

/// Payload for the notification for all epochs
#[derive(Clone)]
pub struct PeriodicMessage;

/// Handler for EpochNotification<EpochMessage>
impl Handler<EpochNotification<EpochMessage>> for BlocksManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EpochMessage>, _ctx: &mut Context<Self>) {
        debug!("Epoch notification received {:?}", msg.checkpoint);
    }
}

/// Handler for EpochNotification<PeriodicMessage>
impl Handler<EpochNotification<PeriodicMessage>> for BlocksManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<PeriodicMessage>, _ctx: &mut Context<Self>) {
        debug!("Periodic epoch notification received {:?}", msg.checkpoint);
    }
}
