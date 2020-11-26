use actix::prelude::*;

use crate::actors::worker;
use crate::{model, types};

pub struct SignData {
    pub wallet: types::SessionWallet,
    pub data: String,
    pub extended_pk: bool,
}

impl Message for SignData {
    type Result = worker::Result<model::ExtendedKeyedSignature>;
}

impl Handler<SignData> for worker::Worker {
    type Result = <SignData as Message>::Result;

    fn handle(
        &mut self,
        SignData {
            wallet,
            data,
            extended_pk,
        }: SignData,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.sign_data(&wallet, &data, extended_pk)
    }
}
