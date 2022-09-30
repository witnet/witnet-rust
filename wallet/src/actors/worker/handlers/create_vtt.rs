use actix::prelude::*;

use crate::{actors::worker, types};
use witnet_data_structures::{fee::AbsoluteFee, transaction::Transaction};

pub struct CreateVtt {
    pub wallet: types::SessionWallet,
    pub params: types::VttParams,
}

pub struct CreateVttResponse {
    pub fee: AbsoluteFee,
    pub transaction: Transaction,
}

impl Message for CreateVtt {
    type Result = worker::Result<CreateVttResponse>;
}

impl Handler<CreateVtt> for worker::Worker {
    type Result = <CreateVtt as Message>::Result;

    fn handle(
        &mut self,
        CreateVtt { wallet, params }: CreateVtt,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.create_vtt(&wallet, params)
            .map(|(transaction, fee)| CreateVttResponse { fee, transaction })
    }
}
