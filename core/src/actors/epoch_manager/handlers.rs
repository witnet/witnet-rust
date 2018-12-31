use actix::Handler;

use log::{debug, error};

use super::{
    messages::{EpochResult, GetEpoch, SubscribeAll, SubscribeEpoch},
    EpochManager,
};

use witnet_data_structures::chain::Epoch;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////
impl Handler<GetEpoch> for EpochManager {
    type Result = EpochResult<Epoch>;

    /// Method to get the last checkpoint (current epoch)
    fn handle(&mut self, _msg: GetEpoch, _ctx: &mut Self::Context) -> EpochResult<Epoch> {
        // Get the last checkpoint (current epoch)
        let checkpoint = self.current_epoch();
        checkpoint
            .as_ref()
            .map(|checkpoint| debug!("Asked for current epoch (#{})", checkpoint))
            .unwrap_or_else(|error| {
                error!(
                    "Failed to retrieve epoch when asked to. Error was: {:?}",
                    error
                )
            });
        checkpoint
    }
}

impl Handler<SubscribeEpoch> for EpochManager {
    type Result = ();

    /// Method to handle SubscribeEpoch messages
    fn handle(&mut self, msg: SubscribeEpoch, _ctx: &mut Self::Context) {
        debug!("New subscription to checkpoint #{:?}", msg.checkpoint);

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
        debug!("New subscription to every checkpoint");

        // Store subscription to all checkpoints
        self.subscriptions_all.push(msg.notification);
    }
}
