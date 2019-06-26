use actix::prelude::*;

use crate::actors::App;
use crate::api;

impl Message for api::CreateWalletRequest {
    type Result = Result<api::CreateWalletResponse, api::Error>;
}

impl Handler<api::CreateWalletRequest> for App {
    type Result = ResponseActFuture<Self, api::CreateWalletResponse, api::Error>;

    fn handle(&mut self, req: api::CreateWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let validated_params = api::validate_create_wallet(req).map_err(api::validation_error);

        let f = fut::result(validated_params)
            .and_then(|params, slf: &mut Self, _ctx| {
                slf.create_wallet(params)
                    .map_err(|err, _slf, _ctx| api::internal_error(err))
            })
            .map(|wallet_id, _slf, _ctx| api::CreateWalletResponse { wallet_id });

        Box::new(f)
    }
}
