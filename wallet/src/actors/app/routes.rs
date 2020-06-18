use futures::{future, Future};
use jsonrpc_core::{Middleware, Params};
use jsonrpc_pubsub::{PubSubHandler, PubSubMetadata, Subscriber};
use serde_json::json;

use super::*;

/// Helper macro to add multiple JSON-RPC methods at once
macro_rules! routes {
    ($io:expr, $api:expr $(,)?) => {};
    ($io:expr, $api:expr, ($wiki:expr, $method_jsonrpc:expr, $actor_msg:ty $(,)?), $($args:tt)*) => {
        {
            let api_addr = $api.clone();
            $io.add_method($method_jsonrpc, move |params: Params| {
                log::debug!("Handling request for method: {}", $method_jsonrpc);
                let addr = api_addr.clone();
                // Try to parse the request params into the actor message
                future::result(params.parse::<$actor_msg>())
                    .map_err(|mut err| {
                        err.data = Some(json!({
                            "schema": format!("https://github.com/witnet/witnet-rust/wiki/{}", $wiki)
                        }));

                        err
                    })
                    .and_then(move |msg| {
                        log::trace!("=> Handling Request: {:?}", &msg);
                        // Then send the parsed message to the actor
                        addr.send(msg)
                            .flatten()
                            .and_then(
                                |x|
                                future::result(serde_json::to_value(x))
                                    .map_err(internal_error)
                            )
                            .map_err(|err| err.into())
                    })
            });
        }
        routes!($io, $api, $($args)*);
    };
}

/// Macro to add multiple JSON-RPC methods that forward the request to the Node at once
macro_rules! forwarded_routes {
    ($io:expr, $api:expr $(,)?) => {};
    ($io:expr, $api:expr, ($method_wallet:expr, $method_node:expr), $($args:tt)*) => {
        {
            let api_addr = $api.clone();
            $io.add_method($method_wallet, move |params: Params| {
                log::debug!("Forwarding request for method: {}", $method_wallet);
                let msg = ForwardRequest {
                    method: $method_node.to_string(),
                    params
                };
                api_addr.send(msg)
                    .flatten()
                    .and_then(|x| {
                        future::result(serde_json::to_value(x)).map_err(internal_error)
                    })
                    .map_err(|err| err.into())
            });
        }
        forwarded_routes!($io, $api, $($args)*);
    };
}

pub fn connect_routes<T, S>(
    handler: &mut PubSubHandler<T, S>,
    api: Addr<App>,
    system_arbiter: Arbiter,
) where
    T: PubSubMetadata,
    S: Middleware<T>,
{
    handler.add_subscription(
        "notifications",
        ("rpc.on", {
            let addr = api.clone();
            move |params: Params, _meta, subscriber: Subscriber| {
                let addr_subscription_id = addr.clone();
                let addr_subscribe = addr.clone();
                let f = future::result(params.parse::<SubscribeRequest>())
                    .then(move |result| match result {
                        Ok(request) => {
                            log::info!("New WS notifications subscriber for session {}", request.session_id);

                            future::Either::A({
                                addr_subscription_id.send(NextSubscriptionId(request.session_id.clone()))
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
                                                    Subscribe(
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
                        })},
                        Err(mut err) =>
                            future::Either::B(subscriber.reject_async({
                                log::trace!("invalid subscription params");

                                err.data = Some(json!({
                                    "schema": "https://github.com/witnet/witnet-rust/wiki/Subscribe-Notifications".to_string()
                                }));
                                err
                            }))
                    });

                system_arbiter.send(f);
            }
        }),
        ("rpc.off", {
            let addr = api.clone();
            move |subscription_id, _meta| {
                addr.send(UnsubscribeRequest(subscription_id))
                    .flatten()
                    .map(|()| json!(()))
                    .map_err(|err| err.into())
            }
        }),
    );

    forwarded_routes!(
        handler,
        api,
        ("get_block", "getBlock"),
        ("get_block_chain", "getBlockChain"),
        ("get_output", "getOutput"),
        ("inventory", "inventory"),
    );

    routes!(
        handler,
        api,
        ("Get-Wallet-Infos", "get_wallet_infos", WalletInfosRequest),
        (
            "Create-Mnemonics",
            "create_mnemonics",
            CreateMnemonicsRequest
        ),
        (
            "Validate-Mnemonics",
            "validate_mnemonics",
            ValidateMnemonicsRequest
        ),
        ("Create-Wallet", "create_wallet", CreateWalletRequest),
        ("Update-Wallet", "update_wallet", UpdateWalletRequest),
        ("Lock-Wallet", "lock_wallet", LockWalletRequest),
        ("Unlock-Wallet", "unlock_wallet", UnlockWalletRequest),
        ("Close-Session", "close_session", CloseSessionRequest),
        ("Get-Balance", "get_balance", GetBalanceRequest),
        (
            "Get-Transactions",
            "get_transactions",
            GetTransactionsRequest
        ),
        (
            "Send-Transaction",
            "send_transaction",
            SendTransactionRequest
        ),
        (
            "Generate-Address",
            "generate_address",
            GenerateAddressRequest
        ),
        ("Get-Addresses", "get_addresses", GetAddressesRequest),
        (
            "Create-Data-Request",
            "create_data_request",
            CreateDataReqRequest
        ),
        ("Create-Vtt", "create_vtt", CreateVttRequest),
        ("Run-Rad-Request", "run_rad_request", RunRadReqRequest),
        ("Set", "set", SetRequest),
        ("Get", "get", GetRequest),
        ("Sign-Data", "sign_data", SignDataRequest),
        ("Shutdown", "shutdown", ShutdownRequest),
    );
}
