//! Stream of Witnet events

use crate::{actors::WitnetBlock, config::Config};
use async_jsonrpc_client::{futures::Stream, DuplexTransport, Transport};
use futures::{future::Either, sink::Sink};
use log::*;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::{prelude::FutureExt, sync::mpsc};
use web3::futures::Future;
use witnet_data_structures::chain::Block;

/// Stream of Witnet events
/// This function returns a future which has a nested future inside.
/// This is because we want to be able to exit the process in the case when
/// we fail to connect to the node, so we await on the outer future, and in
/// the error case we exit the main function.
pub fn witnet_block_stream(
    config: Arc<Config>,
    tx: mpsc::Sender<WitnetBlock>,
) -> (
    async_jsonrpc_client::transports::shared::EventLoopHandle,
    impl Future<Item = impl Future<Item = (), Error = ()>, Error = String>,
) {
    let witnet_addr = config.witnet_jsonrpc_addr.to_string();
    let witnet_addr1 = witnet_addr.clone();
    let witnet_addr2 = witnet_addr.clone();
    // Important: the handle cannot be dropped, otherwise the client stops
    // processing events
    let (handle, witnet_client) =
        async_jsonrpc_client::transports::tcp::TcpSocket::new(&witnet_addr).unwrap();
    let witnet_client1 = witnet_client.clone();

    let fut = witnet_client
        .execute("witnet_subscribe", json!(["newBlocks"]))
        .timeout(Duration::from_secs(1))
        .map_err(move |e| {
            if e.is_elapsed() {
                error!(
                    "Timeout when trying to connect to witnet node at {}",
                    witnet_addr2
                );
                error!("Is the witnet node running?");
            } else if e.is_inner() {
                error!(
                    "Error connecting to witnet node at {}: {:?}",
                    witnet_addr1,
                    e.into_inner()
                );
            } else {
                error!("Unhandled timeout error: {:?}", e);
            }
        })
        .then(|witnet_subscription_id_value| {
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
                        "Failed to subscribe to newBlocks from witnet node".to_string(),
                    );
                }
            };
            info!(
                "Subscribed to witnet newBlocks with subscription id \"{}\"",
                witnet_subscription_id
            );

            let witnet_client = witnet_client1;

            futures::finished(
                witnet_client
                    .subscribe(&witnet_subscription_id.into())
                    .map_err(|e| error!("witnet notification error = {:?}", e))
                    .and_then(move |value| {
                        let tx1 = tx.clone();
                        match serde_json::from_value::<Block>(value) {
                            Ok(block) => {
                                debug!("Got witnet block: {:?}", block);
                                Either::A(
                                    tx1.send(WitnetBlock::New(block))
                                        .map_err(|e| {
                                            error!(
                                                "Failed to send WitnetBlock::New message: {:?}",
                                                e
                                            )
                                        })
                                        .map(|_| ()),
                                )
                            }
                            Err(e) => {
                                error!("Error parsing witnet block: {:?}", e);
                                Either::B(futures::finished(()))
                            }
                        }
                    })
                    .for_each(|_| Ok(())),
            )
        });

    (handle, fut)
}
