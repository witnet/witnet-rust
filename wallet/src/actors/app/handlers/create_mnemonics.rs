use actix::prelude::*;

use crate::actors::App;
use crate::api;

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
        let mnemonics = self.generate_mnemonics(params);

        Ok(api::CreateMnemonicsResponse { mnemonics })
    }
}
