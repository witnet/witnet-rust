use actix::prelude::*;

use crate::{actors::worker, types};
use witnet_data_structures::{fee::AbsoluteFee, transaction::Transaction};

pub struct CreateDataReq {
    pub wallet: types::SessionWallet,
    pub params: types::DataReqParams,
}

pub struct CreateDataReqResponse {
    pub fee: AbsoluteFee,
    pub transaction: Transaction,
}

impl Message for CreateDataReq {
    type Result = worker::Result<CreateDataReqResponse>;
}

impl Handler<CreateDataReq> for worker::Worker {
    type Result = <CreateDataReq as Message>::Result;

    fn handle(
        &mut self,
        CreateDataReq { wallet, params }: CreateDataReq,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.create_data_req(&wallet, params)
            .map(|(transaction, fee)| CreateDataReqResponse { fee, transaction })
    }
}
