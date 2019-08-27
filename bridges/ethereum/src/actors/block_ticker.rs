//! Periodically check for blocks in block relay

use crate::{config::Config, eth::EthState};
use async_jsonrpc_client::futures::Stream;
use futures::{future::Either, sink::Sink};
use log::*;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, sync::oneshot, timer::Interval};
use web3::{
    contract,
    futures::{future, Future},
    types::U256,
};

/// Periodically check for blocks in block relay.
/// Users can send a message consisting of (block_hash, oneshot_tx) and when
/// this actor detects that the block has been posted to the block relay, it
/// will send a notification through the oneshot channel.
/// To create a oneshot channel:
///
/// ```norun
/// let (oneshot_tx, oneshot_rx) = oneshot::channel();
/// let f = block_ticker_tx.send((block_hash, tx)).and_then(|()| rx);
/// ```
///
/// And f is a future that will resolve when the block with hash `block_hash`
/// is included into the block relay.
pub fn block_ticker(
    config: &Config,
    eth_state: Arc<EthState>,
) -> (
    mpsc::UnboundedSender<(U256, oneshot::Sender<()>)>,
    impl Future<Item = (), Error = ()>,
) {
    let eth_account = config.eth_account;
    // Used for wait_for_witnet_block_in_block_relay implementation
    let (self_tx, rx) = mpsc::unbounded_channel();
    let return_tx = self_tx.clone();
    // Map of subscriptions to block hashes. When one of this blocks is posted
    // to the block relay, this actor will send a notification through the
    // corresponding oneshot channel
    let block_hashes: futures_locks::RwLock<HashMap<U256, oneshot::Sender<()>>> =
        futures_locks::RwLock::new(HashMap::new());
    let block_ticker = Interval::new(
        Instant::now(),
        Duration::from_millis(config.block_relay_polling_rate_ms),
    )
    .map_err(|e| error!("Error creating interval: {:?}", e))
    .map(Either::A)
    .select(
        rx.map_err(|e| error!("Error receiving from block ticker channel: {:?}", e))
            .map(Either::B),
    )
    .and_then(move |x| block_hashes.write().map(|block_hashes| (block_hashes, x)))
    .and_then(move |(mut block_hashes, x)| match x {
        Either::A(_instant) => {
            debug!("BlockRelay tick");
            let mut futs = Vec::with_capacity(block_hashes.len());
            for (block_hash, tx) in block_hashes.drain() {
                let self_tx = self_tx.clone();
                let fut = eth_state
                    .block_relay_contract
                    .query(
                        "readDrMerkleRoot",
                        (block_hash,),
                        eth_account,
                        contract::Options::default(),
                        None,
                    )
                    .then(move |x: Result<U256, _>| match x {
                        Ok(_res) => {
                            debug!(
                                "Block {:x} was included in BlockRelay contract!",
                                block_hash
                            );
                            tx.send(()).unwrap();

                            Either::A(futures::finished(()))
                        }
                        Err(e) => {
                            debug!(
                                "Block {:x} not yet included in BlockRelay contract: {:?}",
                                block_hash, e
                            );

                            Either::B(
                                self_tx
                                    .send((block_hash, tx))
                                    .map_err(|e| {
                                        error!("Error sending message to BlockTicker: {:?}", e)
                                    })
                                    .map(|_| debug!("Successfully sent message to BlockTicker")),
                            )
                        }
                    })
                    .map(|_| ());
                futs.push(fut);
            }

            Either::A(future::join_all(futs).map(|_| ()))
        }
        Either::B((block_hash, tx)) => {
            debug!("BlockTicker got new subscription to {:x}", block_hash);
            // Only insert subscription if it did not already exist, to prevent overwriting
            // older subscriptions
            block_hashes.entry(block_hash).or_insert(tx);

            Either::B(futures::finished(()))
        }
    })
    .for_each(|_| Ok(()));

    (return_tx, block_ticker)
}
