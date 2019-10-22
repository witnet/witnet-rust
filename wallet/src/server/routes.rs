use futures::future;
use jsonrpc_core as rpc;
use jsonrpc_pubsub as pubsub;

use witnet_net::server::ws::PubSubHandler;

use super::*;
use super::{dispatch, handlers::Handler as _};

pub fn handler(api: api::Api) -> PubSubHandler {
    let mut handler = pubsub::PubSubHandler::new(rpc::MetaIoHandler::default());

    handler.add_method(
        "getWalletInfos",
        dispatch!(api, requests::GetWalletInfos => responses::WalletInfos),
    );
    handler.add_method(
        "createMnemonics",
        dispatch!(api, requests::CreateMnemonics => responses::Mnemonics),
    );
    handler.add_method(
        "runRadRequest",
        dispatch!(api, requests::RunRadRequest => responses::RadRequestResult),
    );
    handler.add_method(
        "createWallet",
        dispatch!(api, requests::CreateWallet => responses::WalletId),
    );

    handler
}
