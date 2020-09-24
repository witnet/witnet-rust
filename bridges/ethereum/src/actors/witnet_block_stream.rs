//! Stream of Witnet events

use crate::{
    actors::{SuperBlockNotification, WitnetSuperBlock},
    config::Config,
};
use async_jsonrpc_client::{futures::Stream, DuplexTransport, Transport};
use futures::{future::Either, sink::Sink};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::{prelude::FutureExt, sync::mpsc};
use web3::futures::Future;

/// Stream of Witnet events
/// This function returns a future which has a nested future inside.
/// This is because we want to be able to exit the process in the case when
/// we fail to connect to the node, so we await on the outer future, and in
/// the error case we exit the main function.
pub fn witnet_block_stream(
    config: Arc<Config>,
    tx: mpsc::Sender<WitnetSuperBlock>,
) -> (
    async_jsonrpc_client::transports::shared::EventLoopHandle,
    impl Future<Item = impl Future<Item = (), Error = ()>, Error = String>,
) {
    let witnet_addr = config.witnet_jsonrpc_addr.to_string();
    log::info!("Connecting to witnet node at {}", witnet_addr);
    // Important: the handle cannot be dropped, otherwise the client stops
    // processing events
    let (handle, witnet_client) =
        async_jsonrpc_client::transports::tcp::TcpSocket::new(&witnet_addr).unwrap();
    let witnet_client = Arc::new(witnet_client);

    let fut = witnet_client
        .execute("witnet_subscribe", json!(["superblocks"]))
        .timeout(Duration::from_secs(1))
        .map_err(move |e| {
            if e.is_elapsed() {
                log::error!(
                    "Timeout when trying to connect to witnet node at {}",
                    witnet_addr
                );
                log::error!("Is the witnet node running?");
            } else if e.is_inner() {
                log::error!(
                    "Error connecting to witnet node at {}: {:?}",
                    witnet_addr,
                    e.into_inner()
                );
            } else {
                log::error!("Unhandled timeout error: {:?}", e);
            }
        })
        .then(move |witnet_subscription_id_value| {
            // Panic if the subscription wasn't successful
            let witnet_subscription_id = match witnet_subscription_id_value {
                Ok(serde_json::Value::String(s)) => s,
                Ok(x) => {
                    return futures::failed(format!(
                        "Witnet subscription id must be a string, is {:?}",
                        x
                    ));
                }
                Err(_) => {
                    return futures::failed(
                        "Failed to subscribe to blocks from witnet node".to_string(),
                    );
                }
            };
            log::info!(
                "Subscribed to witnet blocks with subscription id \"{}\"",
                witnet_subscription_id
            );

            futures::finished(
                witnet_client
                    .subscribe(&witnet_subscription_id.into())
                    .map_err(|e| log::error!("witnet notification error = {:?}", e))
                    .and_then(move |value| {
                        match serde_json::from_value::<SuperBlockNotification>(value) {
                            Ok(superblock_notification) => {
                                log::debug!(
                                    "Got witnet superblock: {:?}",
                                    superblock_notification.superblock
                                );
                                Either::A(
                                    tx.clone()
                                        .send(WitnetSuperBlock::New(superblock_notification))
                                        .map_err(|e| {
                                            log::error!(
                                                "Failed to send WitnetBlock::New message: {:?}",
                                                e
                                            )
                                        })
                                        .map(|_| ()),
                                )
                            }
                            Err(e) => {
                                log::error!("Error parsing witnet superblock: {:?}", e);
                                Either::B(futures::finished(()))
                            }
                        }
                    })
                    .for_each(|_| Ok(())),
            )
        });

    (handle, fut)
}
