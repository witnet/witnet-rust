use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::str;

use crate::{actors::app, types};
use futures_util::FutureExt;
use witnet_futures_utils::ActorFutureExt2;

/// Create Wallet request, where name, description and overwrite are optional and backup_password
/// is only needed if seed_source is xprv
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateWalletRequest {
    name: Option<String>,
    description: Option<String>,
    password: types::Password,
    seed_source: String,
    seed_data: types::Password,
    overwrite: Option<bool>,
    /// only needed if seed_source is xprv
    backup_password: Option<types::Password>,
    birth_date: Option<types::BirthDate>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateWalletResponse {
    pub wallet_id: String,
}

impl Message for CreateWalletRequest {
    type Result = app::Result<CreateWalletResponse>;
}

impl Handler<CreateWalletRequest> for app::App {
    type Result = app::ResponseActFuture<CreateWalletResponse>;

    fn handle(&mut self, req: CreateWalletRequest, _ctx: &mut Self::Context) -> Self::Result {
        let validated_params = app::methods::validate(
            req.password,
            req.seed_data,
            req.seed_source,
            req.name,
            req.description,
            req.overwrite,
            req.backup_password,
            req.birth_date,
        );

        let f = fut::result(validated_params).and_then(|params, slf: &mut Self, _ctx| {
            slf.create_wallet(
                params.password,
                params.seed_source,
                params.name,
                params.description,
                params.overwrite,
                params.birth_date,
            )
            .map(|res| res.map(|wallet_id| CreateWalletResponse { wallet_id }))
            .into_actor(slf)
        });

        Box::pin(f)
    }
}
