use actix::Handler;

use log::{debug, info};

use super::{
    messages::{EpochResult, GetEpoch, SubscribeAll, SubscribeEpoch},
    Epoch, EpochManager,
};

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////
impl Handler<GetEpoch> for EpochManager {
    type Result = EpochResult<Epoch>;

    /// Method to get the last checkpoint (current epoch)
    fn handle(&mut self, _msg: GetEpoch, _ctx: &mut Self::Context) -> EpochResult<Epoch> {
        // Get the last checkpoint (current epoch)
        let checkpoint = self.current_epoch();
        debug!("Current epoch: {:?}", checkpoint);
        checkpoint
    }
}

impl Handler<SubscribeEpoch> for EpochManager {
    type Result = ();

    /// Method to handle SubscribeEpoch messages
    fn handle(&mut self, msg: SubscribeEpoch, _ctx: &mut Self::Context) {
        info!("Subscription received to checkpoint {:?}", msg.checkpoint);

        // Store subscription to target checkpoint
        self.subscriptions_epoch
            .entry(msg.checkpoint)
            .or_insert_with(|| vec![])
            .push(msg.notification);
    }
}

impl Handler<SubscribeAll> for EpochManager {
    type Result = ();

    /// Method to handle SubscribeAll messages
    fn handle(&mut self, msg: SubscribeAll, _ctx: &mut Self::Context) {
        info!("Subscription received to all checkpoints");

        // Store subscription to all checkpoints
        self.subscriptions_all.push(msg.notification);
    }
}
