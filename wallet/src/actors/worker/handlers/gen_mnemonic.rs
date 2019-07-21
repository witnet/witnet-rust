use actix::prelude::*;

use crate::actors::worker;
use crate::types;

pub struct GenMnemonic(pub types::MnemonicLength);

impl Message for GenMnemonic {
    type Result = String;
}

impl Handler<GenMnemonic> for worker::Worker {
    type Result = <GenMnemonic as Message>::Result;

    fn handle(
        &mut self,
        GenMnemonic(length): GenMnemonic,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.gen_mnemonic(length)
    }
}
