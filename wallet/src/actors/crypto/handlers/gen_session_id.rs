use std::sync::Arc;

use actix::prelude::*;

use crate::actors::Crypto;
use crate::wallet;

pub struct GenSessionId(pub Arc<wallet::Key>);

impl Message for GenSessionId {
    type Result = String;
}

impl Handler<GenSessionId> for Crypto {
    type Result = <GenSessionId as Message>::Result;

    fn handle(
        &mut self,
        GenSessionId(key): GenSessionId,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.gen_session_id(key.as_ref())
    }
}
