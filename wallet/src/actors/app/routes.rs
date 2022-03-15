use futures::FutureExt;
use jsonrpc_core::{Middleware, Params};
use jsonrpc_pubsub::{PubSubHandler, PubSubMetadata, Subscriber};
use serde_json::json;
use std::future;

use super::*;
use futures_util::compat::{Compat, Compat01As03};
use witnet_futures_utils::TryFutureExt2;

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
                let fut03 = future::ready(params.parse::<$actor_msg>())
                    .then(move |res| match res {
                        Err(mut err) => {
                            err.data = Some(json!({
                                "schema": format!("https://github.com/witnet/witnet-rust/wiki/{}", $wiki)
                            }));

                            futures::future::Either::Left(future::ready(Err(err)))
                        }
                        Ok(msg) => {
                            log::trace!("=> Handling Request: {:?}", &msg);
                            // Then send the parsed message to the actor
                            let f = addr.send(msg)
                                .flatten_err()
                                .map(|res: Result<_>| {
                                    res.and_then(|x| serde_json::to_value(x).map_err(internal_error))
                                        .map_err(|e| e.into())
                                });

                            futures::future::Either::Right(f)
                        }
                    });

                Compat::new(Box::pin(fut03))
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
                let fut03 = api_addr.send(msg)
                    .flatten_err()
                    .map(|res: Result<_>| {
                        res.and_then(|x| serde_json::to_value(x).map_err(internal_error))
                            .map_err(|e| e.into())
                    });
                Compat::new(Box::pin(fut03))
            });
        }
        forwarded_routes!($io, $api, $($args)*);
    };
}

pub fn connect_routes<T, S>(
    handler: &mut PubSubHandler<T, S>,
    api: Addr<App>,
    system_arbiter: ArbiterHandle,
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
                let f = future::ready(params.parse::<SubscribeRequest>())
                    .then(move |result| match result {
                        Ok(request) => {
                            log::info!("New WS notifications subscriber for session {}", request.session_id);

                            futures::future::Either::Left({
                                addr_subscription_id.send(NextSubscriptionId(request.session_id.clone()))
                                    .flatten_err()
                                    .map(|res| res.map_err(|e: Error| e.into()))
                                    .then(move |result| match result {
                                    Ok(subscription_id) => futures::future::Either::Left({
                                        let fut01 = subscriber
                                            .assign_id_async(subscription_id.clone());

                                        Compat01As03::new(fut01)
                                            .map(move |res| res.map_err(|()| {
                                                log::error!("Failed to assign id");
                                            }).map(|sink| {
                                                addr_subscribe.do_send(
                                                    Subscribe(
                                                        request.session_id,
                                                        subscription_id,
                                                        sink
                                                    )
                                                );
                                            }))
                                    }),
                                    Err(err) => futures::future::Either::Right({
                                        let fut01 = subscriber.reject_async(err);
                                        Compat01As03::new(fut01)
                                    })
                                })
                        })},
                        Err(mut err) =>
                            futures::future::Either::Right({
                                let fut01 = subscriber.reject_async({
                                    log::trace!("invalid subscription params");

                                    err.data = Some(json!({
                                    "schema": "https://github.com/witnet/witnet-rust/wiki/Subscribe-Notifications".to_string()
                                }));
                                    err
                                });
                                Compat01As03::new(fut01)
                            })
                    }).map(|_: std::result::Result<(), ()>| ());

                system_arbiter.spawn(Box::pin(f));
            }
        }),
        ("rpc.off", {
            let addr = api.clone();
            move |subscription_id, _meta| {
                let fut03 = addr.send(UnsubscribeRequest(subscription_id))
                    .flatten_err()
                    .map(|res| res.map(|()| json!(())).map_err(|e: Error| e.into()));

                Compat::new(Box::pin(fut03))
            }
        }),
    );

    forwarded_routes!(
        handler,
        api,
        ("data_request_report", "dataRequestReport"),
        ("get_block", "getBlock"),
        ("get_block_chain", "getBlockChain"),
        ("get_output", "getOutput"),
        ("get_transaction_by_hash", "getTransaction"),
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
        ("Delete-Wallet", "delete_wallet", DeleteWalletRequest),
        ("Update-Wallet", "update_wallet", UpdateWalletRequest),
        ("Lock-Wallet", "lock_wallet", LockWalletRequest),
        ("Unlock-Wallet", "unlock_wallet", UnlockWalletRequest),
        ("Resync-Wallet", "resync_wallet", ResyncWalletRequest),
        ("Close-Session", "close_session", CloseSessionRequest),
        ("Refresh-Session", "refresh_session", RefreshSessionRequest),
        ("Get-Balance", "get_balance", GetBalanceRequest),
        ("Get-Utxo-Info", "get_utxo_info", UtxoInfoRequest),
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
        (
            "Export-Master-Key",
            "export_master_key",
            ExportMasterKeyRequest
        ),
        ("Shutdown", "shutdown", ShutdownRequest),
    );
}
