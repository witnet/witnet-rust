use actix::Handler;

use witnet_data_structures::chain::Epoch;

use super::EpochManager;
use crate::actors::{
    epoch_manager::{EpochConstants, EpochManagerError},
    messages::{EpochResult, GetEpoch, GetEpochConstants, SubscribeAll, SubscribeEpoch},
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
        checkpoint
            .as_ref()
            .map(|checkpoint| log::debug!("Asked for current epoch (#{})", checkpoint))
            .unwrap_or_else(|error| match error {
                EpochManagerError::CheckpointZeroInTheFuture(_) => log::debug!(
                    "Failed to retrieve epoch when asked to. Error was: {:?}",
                    error
                ),
                _ => log::error!(
                    "Failed to retrieve epoch when asked to. Error was: {:?}",
                    error
                ),
            });
        checkpoint
    }
}

impl Handler<SubscribeEpoch> for EpochManager {
    type Result = ();

    /// Method to handle SubscribeEpoch messages
    fn handle(&mut self, msg: SubscribeEpoch, _ctx: &mut Self::Context) {
        log::debug!("New subscription to checkpoint #{:?}", msg.checkpoint);

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
        log::debug!("New subscription to every checkpoint");

        // Store subscription to all checkpoints
        self.subscriptions_all.push(msg.notification);
    }
}

impl Handler<GetEpochConstants> for EpochManager {
    type Result = Option<EpochConstants>;

    /// Return a function which can be used to calculate the timestamp for a
    /// checkpoint (the start of an epoch). This assumes that the
    /// checkpoint_zero_timestamp and checkpoints_period constants never change
    fn handle(&mut self, _msg: GetEpochConstants, _ctx: &mut Self::Context) -> Self::Result {
        self.constants
    }
}
