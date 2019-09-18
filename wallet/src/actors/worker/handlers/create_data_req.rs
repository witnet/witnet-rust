use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct CreateDataReq(pub types::SessionWallet, pub types::DataReqParams);

impl Message for CreateDataReq {
    type Result = worker::Result<types::Transaction>;
}

impl Handler<CreateDataReq> for worker::Worker {
    type Result = <CreateDataReq as Message>::Result;

    fn handle(
        &mut self,
        CreateDataReq(wallet, params): CreateDataReq,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.create_data_req(&wallet, params)
    }
}
