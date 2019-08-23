//! Stream of Witnet events

use crate::{actors::ActorMessage, config::Config};
use async_jsonrpc_client::{futures::Stream, DuplexTransport, Transport};
use futures::{future::Either, sink::Sink};
use log::*;
use serde_json::json;
use std::{process, sync::Arc, time::Duration};
use tokio::{prelude::FutureExt, sync::mpsc};
use web3::futures::Future;
use witnet_data_structures::chain::Block;

/// Stream of Witnet events
pub fn witnet_block_stream(
    config: Arc<Config>,
    tx: mpsc::Sender<ActorMessage>,
) -> (
    async_jsonrpc_client::transports::shared::EventLoopHandle,
    impl Future<Item = (), Error = ()>,
) {
    let witnet_addr = config.witnet_jsonrpc_addr.to_string();
    let witnet_addr1 = witnet_addr.clone();
    let witnet_addr2 = witnet_addr.clone();
    // Important: the handle cannot be dropped, otherwise the client stops
    // processing events
    let (handle, witnet_client) =
        async_jsonrpc_client::transports::tcp::TcpSocket::new(&witnet_addr).unwrap();
    let witnet_client1 = witnet_client.clone();

    let fut1 = witnet_client
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
                error!("{:?}", e);
            }
        })
        .then(|witnet_subscription_id_value| {
            // Panic if the subscription wasn't successful
            let witnet_subscription_id = match witnet_subscription_id_value {
                Ok(serde_json::Value::String(s)) => s,
                Ok(x) => {
                    error!("Witnet subscription id must be a string, is {:?}", x);
                    process::exit(1);
                }
                Err(_) => {
                    error!("Failed to subscribe to newBlocks from witnet node");
                    process::exit(1);
                }
            };
            info!(
                "Subscribed to witnet newBlocks with subscription id \"{}\"",
                witnet_subscription_id
            );

            let witnet_client = witnet_client1;

            witnet_client
                .subscribe(&witnet_subscription_id.into())
                .map_err(|e| error!("witnet notification error = {:?}", e))
                .and_then(move |value| {
                    let tx1 = tx.clone();
                    // TODO: get current epoch to distinguish between old blocks that are sent
                    // to us while synchronizing and new blocks
                    match serde_json::from_value::<Block>(value) {
                        Ok(block) => {
                            debug!("Got witnet block: {:?}", block);
                            Either::A(
                                tx1.send(ActorMessage::NewWitnetBlock(Box::new(block)))
                                    .map_err(|_| ())
                                    .map(|_| ()),
                            )
                        }
                        Err(e) => {
                            error!("Error parsing witnet block: {:?}", e);
                            Either::B(futures::finished(()))
                        }
                    }
                })
                .for_each(|_| Ok(()))
        });
    let fut1 = Either::A(fut1);
    let fut2 = Either::B(futures::finished(()));
    let fut = if config.subscribe_to_witnet_blocks {
        fut1
    } else {
        fut2
    };

    (handle, fut)
}
