use actix::prelude::*;

use crate::actors::Crypto;
use crate::types;

pub struct GenSessionId(pub types::Key);

impl Message for GenSessionId {
    type Result = types::SessionId;
}

impl Handler<GenSessionId> for Crypto {
    type Result = <GenSessionId as Message>::Result;

    fn handle(
        &mut self,
        GenSessionId(key): GenSessionId,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.gen_session_id(&key)
    }
}
