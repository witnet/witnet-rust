use actix::prelude::*;
use futures::{future, Future};
use jsonrpc_core::{Middleware, Params};
use jsonrpc_pubsub::{PubSubHandler, PubSubMetadata, Subscriber};
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

pub fn connect_routes<T, S>(
    handler: &mut PubSubHandler<T, S>,
    app: Addr<App>,
    system_arbiter: Arbiter,
) where
    T: PubSubMetadata,
    S: Middleware<T>,
{
    handler.add_subscription(
        "notifications",
        ("subscribeNotifications", {
            let addr = app.clone();
            move |params: Params, _meta, subscriber: Subscriber| {
                let addr_subscription_id = addr.clone();
                let addr_subscribe = addr.clone();
                let f = future::result(params.parse::<api::SubscribeRequest>())
                    .then(move |result| match result {
                        Ok(request) =>
                            future::Either::A({
                                addr_subscription_id.send(api::NextSubscriptionId(request.session_id.clone()))
                                    .flatten()
                                    .map_err(|err| err.into())
                                    .then(move |result| match result {
                                        Ok(subscription_id) => future::Either::A(
                                            subscriber
                                                .assign_id_async(subscription_id.clone())
                                                .map_err(|()| {
                                                    log::error!("Failed to assign id");
                                                })
                                                .and_then(move |sink| {
                                                    addr_subscribe.do_send(
                                                        api::Subscribe(
                                                            request.session_id,
                                                            subscription_id,
                                                            sink
                                                        )
                                                    );
                                                    future::ok(())
                                                })
                                        ),
                                        Err(err) => future::Either::B(
                                            subscriber.reject_async(err)
                                        )
                                    })
                            }),
                        Err(mut err) =>
                            future::Either::B(subscriber.reject_async({
                                log::trace!("invalid subscription params");

                                err.data = Some(json!({
                                    "schema": format!("https://github.com/witnet/witnet-rust/wiki/Subscribe-Notifications")
                                }));
                                err
                            }))
                    });

                system_arbiter.send(f);
            }
        }),
        ("unsubscribeNotifications", {
            let addr = app.clone();
            move |subscription_id, _meta| {
                addr.send(api::UnsubscribeRequest(subscription_id))
                    .flatten()
                    .map(|()| json!(()))
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
            "Send-Transaction",
            "sendTransaction",
            api::SendTransactionRequest
        ),
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
        ("Create-Vtt", "createVttRequest", api::CreateVttRequest),
        ("Run-Rad-Request", "runRadRequest", api::RunRadReqRequest),
        (
            "Send-Data-Request",
            "sendDataRequest",
            api::SendDataReqRequest
        ),
    );
}
