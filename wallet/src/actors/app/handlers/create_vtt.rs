use actix::prelude::*;
use bech32::FromBase32;
use serde::{Deserialize, Serialize};

use crate::actors::app;
use crate::types;
use crate::types::{Hashable as _, ProtobufConvert as _};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVttRequest {
    session_id: types::SessionId,
    wallet_id: String,
    address: String,
    label: Option<String>,
    amount: u64,
    fee: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVttResponse {
    pub transaction_id: String,
    pub transaction: types::Transaction,
    pub bytes: String,
}

impl Message for CreateVttRequest {
    type Result = app::Result<CreateVttResponse>;
}

impl Handler<CreateVttRequest> for app::App {
    type Result = app::ResponseActFuture<CreateVttResponse>;

    fn handle(&mut self, msg: CreateVttRequest, _ctx: &mut Self::Context) -> Self::Result {
        let testnet = self.params.testnet;
        let validated = validate(testnet, &msg.address).map_err(app::validation_error);

        let f = fut::result(validated).and_then(|pkh, slf: &mut Self, _ctx| {
            let params = types::VttParams {
                pkh,
                label: msg.label,
                value: msg.amount,
                fee: msg.fee,
            };

            slf.create_vtt(&msg.session_id, &msg.wallet_id, params)
                .map(|transaction, _, _| {
                    let transaction_id = hex::encode(transaction.hash().as_ref());
                    let bytes = hex::encode(transaction.to_pb_bytes().unwrap());

                    CreateVttResponse {
                        transaction_id,
                        transaction,
                        bytes,
                    }
                })
        });

        Box::new(f)
    }
}

/// Validate `CreateVttRequest`.
///
/// To be valid it must pass these checks:
/// - destination address must be in the same network (test/main)
/// - source account must have enough balance
fn validate(testnet: bool, address: &str) -> Result<types::PublicKeyHash, app::ValidationErrors> {
    let pkh = validate_address(testnet, address)?;

    Ok(pkh)
}

fn validate_address(
    testnet: bool,
    address: &str,
) -> Result<types::PublicKeyHash, app::ValidationErrors> {
    let (prefix, pkh_u5) = bech32::decode(address).map_err(|err| {
        log::warn!("Invalid address: {}", err);
        app::field_error("address", "Address failed to deserialize.")
    })?;
    let pkh_vec = Vec::from_base32(&pkh_u5).map_err(|err| {
        log::warn!("Invalid address: {}", err);
        app::field_error("address", "Address failed to deserialize from base 32.")
    })?;

    if same_network(testnet, &prefix) {
        types::PublicKeyHash::from_bytes(&pkh_vec)
            .map_err(|err| app::field_error("address", format!("{}", err)))
    } else {
        Err(app::field_error(
            "address",
            "Address not in the same network.",
        ))
    }
}

fn same_network(testnet: bool, prefix: &str) -> bool {
    prefix == "twit" && testnet || prefix == "wit" && !testnet
}
