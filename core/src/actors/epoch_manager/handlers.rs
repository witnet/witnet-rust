use actix::Handler;

use log::debug;

use super::{
    messages::{EpochResult, GetEpoch},
    Epoch, EpochManager,
};

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////
impl Handler<GetEpoch> for EpochManager {
    type Result = EpochResult<Epoch>;

    /// Method to get the last checkpoint (current epoch)
    fn handle(&mut self, _msg: GetEpoch, _ctx: &mut Self::Context) -> EpochResult<Epoch> {
        let r = self.current_epoch();
        debug!("Current epoch: {:?}", r);
        r
    }
}
