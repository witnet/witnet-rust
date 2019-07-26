use actix::prelude::*;
use serde::{Deserialize, Serialize};

use crate::actors::app;

#[derive(Debug, Serialize, Deserialize)]
pub struct SendVttRequest {
    pub wallet_id: String,
    pub to_address: Vec<u8>,
    pub amount: u64,
    pub fee: u64,
    pub subject: String,
}

impl Message for SendVttRequest {
    type Result = app::Result<()>;
}

impl Handler<SendVttRequest> for app::App {
    type Result = <SendVttRequest as Message>::Result;

    fn handle(&mut self, _msg: SendVttRequest, _ctx: &mut Self::Context) -> Self::Result {
        Ok(())
    }
}
