use actix::{Context, Handler};

use witnet_data_structures::chain::Epoch;

use super::EpochManager;
use crate::actors::{
    epoch_manager::{EpochConstants, EpochManagerError},
    messages::{
        EpochResult, GetEpoch, GetEpochConstants, SetEpochConstants, SubscribeAll, SubscribeEpoch,
    },
};
use witnet_util::timestamp::get_timestamp;

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
            .map(|checkpoint| log::trace!("Asked for current epoch (#{})", checkpoint))
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
        log::trace!("New subscription to checkpoint #{:?}", msg.checkpoint);

        // Store subscription to target checkpoint
        self.subscriptions_epoch
            .entry(msg.checkpoint)
            .or_default()
            .push(msg.notification);
    }
}

impl Handler<SubscribeAll> for EpochManager {
    type Result = ();

    /// Method to handle SubscribeAll messages
    fn handle(&mut self, msg: SubscribeAll, _ctx: &mut Self::Context) {
        log::trace!("New subscription to every checkpoint");

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

impl Handler<SetEpochConstants> for EpochManager {
    type Result = ();

    fn handle(&mut self, msg: SetEpochConstants, _ctx: &mut Context<Self>) -> Self::Result {
        // Check if the epoch calculated with the current version of the epoch constants
        // and the last_checked_epoch are different and if they are, subtract that difference
        // from the new last_checked_epoch.
        let current_time = get_timestamp();
        let epoch_before_update = msg
            .epoch_constants
            .epoch_at(current_time)
            .unwrap_or_default();
        let epoch_diff =
            epoch_before_update.saturating_sub(self.last_checked_epoch.unwrap_or_default());

        self.constants = Some(msg.epoch_constants);

        self.last_checked_epoch = Some(
            msg.epoch_constants
                .epoch_at(current_time)
                .unwrap_or_default()
                .saturating_sub(epoch_diff),
        );
    }
}
