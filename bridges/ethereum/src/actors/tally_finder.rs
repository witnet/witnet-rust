//! Periodically ask the Witnet node for resolved data requests

use crate::{actors::WitnetBlock, config::Config, eth::EthState};
use async_jsonrpc_client::{futures::Stream, Transport};
use futures::{future::Either, sink::Sink};
use log::*;
use rand::{thread_rng, Rng};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, timer::Interval};
use web3::futures::Future;
use witnet_data_structures::{chain::Block, chain::DataRequestInfo};

/// Periodically ask the Witnet node for resolved data requests
pub fn tally_finder(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
    tx: mpsc::Sender<WitnetBlock>,
) -> (
    async_jsonrpc_client::transports::shared::EventLoopHandle,
    impl Future<Item = (), Error = ()>,
) {
    let witnet_addr = config.witnet_jsonrpc_addr.to_string();
    // Important: the handle cannot be dropped, otherwise the client stops
    // processing events
    let (handle, witnet_client) =
        async_jsonrpc_client::transports::tcp::TcpSocket::new(&witnet_addr).unwrap();
    let witnet_client1 = witnet_client.clone();

    (handle, Interval::new(Instant::now(), Duration::from_millis(config.witnet_dr_report_polling_rate_ms))
        .map_err(|e| error!("Error creating interval: {:?}", e))
        .and_then(move |x| eth_state.wrb_requests.read().map(move |wrb_requests| (wrb_requests, x)))
        .and_then(move |(wrb_requests, _instant)| {
            debug!("Report tick");
            // Try to get the report of a random data request, maybe it already was resolved
            let included = wrb_requests.included();
            debug!("Included data requests: {:?}", included);
            if included.is_empty() {
                return Either::A(futures::failed(()));
            }
            let i = thread_rng().gen_range(0, included.len());
            let (dr_id, dr_tx_hash) = included.iter().nth(i).unwrap();
            debug!("[{}] Report ticker will check data request {}", dr_id, dr_tx_hash);

            Either::B(witnet_client
                .execute("dataRequestReport", json!([*dr_tx_hash]))
                .map_err(|e| error!("dataRequestReport: {:?}", e))
            )
        })
        .and_then(move |report| {
            debug!("dataRequestReport: {}", report);

            match serde_json::from_value::<Option<DataRequestInfo>>(report) {
                Ok(Some(DataRequestInfo { block_hash_tally_tx: Some(block_hash_tally_tx), .. })) => {
                    info!("Found possible tally to be reported from an old witnet block {}", block_hash_tally_tx);
                    Either::A(witnet_client1.execute("getBlock", json!([block_hash_tally_tx]))
                        .map_err(|e| error!("getBlock: {:?}", e)))
                }
                Ok(..) => {
                    // No problem, this means the data request has not been resolved yet
                    debug!("Data request not resolved yet");
                    Either::B(futures::failed(()))
                }
                Err(e) => {
                    error!("dataRequestReport deserialize error: {:?}", e);
                    Either::B(futures::failed(()))
                }
            }
        })
        .and_then(move |value| {
            match serde_json::from_value::<Block>(value) {
                Ok(block) => {
                    debug!("Replaying an old witnet block so that we can report the resolved data requests: {:?}", block);
                    Either::A(
                        tx.clone().send(WitnetBlock::Replay(block))
                            .map_err(|e| error!("Failed to send WitnetBlock::Replay: {:?}", e))
                            .map(|_| ()),
                    )
                }
                Err(e) => {
                    error!("Error parsing witnet block: {:?}", e);
                    Either::B(futures::finished(()))
                }
            }

        })
        .then(|_| Ok(()))
        .for_each(|_| Ok(())))
}
