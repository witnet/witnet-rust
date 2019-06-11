use actix::prelude::*;

use crate::actors::App;
use crate::api;
use witnet_crypto as crypto;

impl Message for api::CreateMnemonicsRequest {
    type Result = Result<api::CreateMnemonicsResponse, failure::Error>;
}

impl Handler<api::CreateMnemonicsRequest> for App {
    type Result = Result<api::CreateMnemonicsResponse, failure::Error>;

    fn handle(
        &mut self,
        api::CreateMnemonicsRequest { length }: api::CreateMnemonicsRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        log::debug!("Generating mnemonics with length: {:?}", length);
        let mnemonic = crypto::mnemonic::MnemonicGen::new()
            .with_len(length)
            .generate();
        let words = mnemonic.words();
        let mnemonics = api::CreateMnemonicsResponse {
            mnemonics: words.to_string(),
        };

        Ok(mnemonics)
    }
}
