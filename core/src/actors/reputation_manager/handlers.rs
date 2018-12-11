use super::{messages, ReputationManager};
use actix::{Handler, Message};

impl Handler<messages::ValidatePoE> for ReputationManager {
    type Result = <messages::ValidatePoE as Message>::Result;

    fn handle(&mut self, _msg: messages::ValidatePoE, _ctx: &mut Self::Context) -> Self::Result {
        // TODO: This implementation is dummy. The real implementation
        // should be done in the future. See
        // https://github.com/witnet/witnet-rust/issues/235
        true
    }
}
