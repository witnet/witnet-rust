use actix::prelude::*;
use futures::{future, Future};
use jsonrpc_core::{Middleware, Params};
use jsonrpc_pubsub::{PubSubHandler, PubSubMetadata};
use serde_json::json;

use crate::actors::app::App;
use crate::api;

/// Helper macro to add multiple JSON-RPC methods at once
macro_rules! routes {
    ($io:expr, $app:expr $(,)?) => {};
    ($io:expr, $app:expr, ($wiki:expr, $method_jsonrpc:expr, $actor_msg:ty $(,)?), $($args:tt)*) => {
        {
            let app_addr = $app.clone();
            $io.add_method($method_jsonrpc, move |params: Params| {
                log::debug!("Handling request for method: {}", $method_jsonrpc);
                let addr = app_addr.clone();
                // Try to parse the request params into the actor message
                future::result(params.parse::<$actor_msg>())
                    .map_err(|mut err| {
                        err.data = Some(json!({
                            "schema": format!("https://github.com/witnet/witnet-rust/wiki/{}", $wiki)
                        }));

                        err
                    })
                    .and_then(move |msg| {
                        // Then send the parsed message to the actor
                        addr.send(msg)
                            .flatten()
                            .and_then(
                                |x|
                                future::result(serde_json::to_value(x)).map_err(api::internal_error)
                            )
                            .map_err(|err| err.into())
                    })
            });
        }
        routes!($io, $app, $($args)*);
    };
}

/// Macro to add multiple JSON-RPC methods that forward the request to the Node at once
macro_rules! forwarded_routes {
    ($io:expr, $app:expr $(,)?) => {};
    ($io:expr, $app:expr, $method:expr, $($args:tt)*) => {
        {
            let app_addr = $app.clone();
            $io.add_method($method, move |params: Params| {
                log::debug!("Forwarding request for method: {}", $method);
                let msg = api::ForwardRequest {
                    method: $method.to_string(),
                    params
                };
                app_addr.send(msg)
                    .flatten()
                    .and_then(|x| {
                        future::result(serde_json::to_value(x)).map_err(api::internal_error)
                    })
                    .map_err(|err| err.into())
            });
        }
        forwarded_routes!($io, $app, $($args)*);
    };
}

pub fn connect_routes<T, S>(handler: &mut PubSubHandler<T, S>, app: Addr<App>)
where
    T: PubSubMetadata,
    S: Middleware<T>,
{
    handler.add_subscription(
        "notifications",
        ("subscribeNotifications", {
            let addr = app.clone();
            move |_, _, subscriber| addr.do_send(api::SubscribeRequest(subscriber))
        }),
        ("unsubscribeNotifications", {
            let addr = app.clone();
            move |id, _| {
                addr.send(api::UnsubscribeRequest(id))
                    .flatten()
                    .and_then(|_| future::ok(json!({"status": "ok"})))
                    .map_err(|err| err.into())
            }
        }),
    );

    forwarded_routes!(
        handler,
        app,
        "getBlock",
        "getBlockChain",
        "getOutput",
        "inventory",
    );

    routes!(
        handler,
        app,
        (
            "Get-Wallet-Infos",
            "getWalletInfos",
            api::WalletInfosRequest
        ),
        (
            "Create-Mnemonics",
            "createMnemonics",
            api::CreateMnemonicsRequest
        ),
        ("Import-Seed", "importSeed", api::ImportSeedRequest),
        ("Create-Wallet", "createWallet", api::CreateWalletRequest),
        ("Lock-Wallet", "lockWallet", api::LockWalletRequest),
        ("Unlock-Wallet", "unlockWallet", api::UnlockWalletRequest),
        ("Lock-Wallet", "lockWallet", api::LockWalletRequest),
        ("Close-Session", "closeSession", api::CloseSessionRequest),
        (
            "Get-Transactions",
            "getTransactions",
            api::GetTransactionsRequest
        ),
        ("Send-Vtt", "sendVTT", api::SendVttRequest),
        (
            "Generate-Address",
            "generateAddress",
            api::GenerateAddressRequest
        ),
        (
            "Create-Data-Request",
            "createDataRequest",
            api::CreateDataReqRequest
        ),
        ("Run-Rad-Request", "runRadRequest", api::RunRadReqRequest),
        (
            "Send-Data-Request",
            "sendDataRequest",
            api::SendDataReqRequest
        ),
    );
}
