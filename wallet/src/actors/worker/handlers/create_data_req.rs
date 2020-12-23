use actix::prelude::*;

use crate::{actors::worker, types};
use witnet_data_structures::transaction::Transaction;

pub struct CreateDataReq {
    pub wallet: types::SessionWallet,
    pub params: types::DataReqParams,
}

impl Message for CreateDataReq {
    type Result = worker::Result<Transaction>;
}

impl Handler<CreateDataReq> for worker::Worker {
    type Result = <CreateDataReq as Message>::Result;

    fn handle(
        &mut self,
        CreateDataReq { wallet, params }: CreateDataReq,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.create_data_req(&wallet, params)
    }
}
