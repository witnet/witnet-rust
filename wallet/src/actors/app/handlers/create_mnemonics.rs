use actix::prelude::*;

use crate::actors::App;
use crate::api;
use witnet_crypto as crypto;

impl Message for api::CreateMnemonicsRequest {
    type Result = Result<api::CreateMnemonicsResponse, api::Error>;
}

impl Handler<api::CreateMnemonicsRequest> for App {
    type Result = Result<api::CreateMnemonicsResponse, api::Error>;

    fn handle(
        &mut self,
        req: api::CreateMnemonicsRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let params = api::validate_create_mnemonics(req).map_err(api::validation_error)?;

        let mnemonic = crypto::mnemonic::MnemonicGen::new()
            .with_len(params.length)
            .generate();
        let words = mnemonic.words();
        let mnemonics = api::CreateMnemonicsResponse {
            mnemonics: words.to_string(),
        };

        Ok(mnemonics)
    }
}
