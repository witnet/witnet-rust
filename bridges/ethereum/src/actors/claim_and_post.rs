//! Actor which tries to claim data requests from WRB and posts them to Witnet

use crate::{actors::handle_receipt, actors::ClaimMsg, config::Config, eth::EthState};
use async_jsonrpc_client::{futures::Stream, transports::tcp::TcpSocket, Transport};
use ethabi::{Bytes, Token};
use futures::{future::Either, sink::Sink};
use log::*;
use rand::{thread_rng, Rng};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, timer::Interval};
use web3::{contract, futures::Future, types::U256};
use witnet_crypto::hash::{calculate_sha256, Sha256};
use witnet_data_structures::{
    chain::{DataRequestOutput, Hashable},
    proto::ProtobufConvert,
};
use witnet_validations::validations::validate_rad_request;

fn convert_json_array_to_eth_bytes(value: Value) -> Result<Bytes, serde_json::Error> {
    // Convert json values such as [1, 2, 3] into bytes
    serde_json::from_value(value)
}

type ClaimDataRequestsParams = (Vec<U256>, [U256; 4], [U256; 2], [U256; 2], [U256; 4], Bytes);

/// Check if we can claim a DR from the WRB locally,
/// without sending any transactions to Ethereum,
/// and return all the parameters needed for the real transaction
fn try_to_claim_local_query(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
    witnet_client: Arc<TcpSocket>,
    dr_id: U256,
) -> impl Future<Item = (DataRequestOutput, ClaimDataRequestsParams), Error = ()> {
    let eth_account = config.eth_account;

    let wrb_contract = eth_state.wrb_contract.clone();
    let wrb_contract2 = wrb_contract.clone();
    let wrb_contract3 = wrb_contract.clone();
    let wrb_contract5 = wrb_contract.clone();
    let wrb_contract6 = wrb_contract.clone();
    let wrb_contract7 = wrb_contract.clone();
    let witnet_client = Arc::clone(&witnet_client);
    let witnet_client2 = Arc::clone(&witnet_client);
    let witnet_client3 = Arc::clone(&witnet_client);
    let witnet_client4 = Arc::clone(&witnet_client);

    wrb_contract
        .query(
            "checkDataRequestsClaimability",
            (vec![dr_id],),
            eth_account,
            contract::Options::default(),
            None,
        )
        .map_err(|e| error!("checkDataRequestsClaimability {:?}", e))
        .and_then(move |claimable: Vec<bool>| {
            match claimable.get(0) {
                Some(true) => {
                    Either::A(wrb_contract
                        .query(
                            "readDataRequest",
                            (dr_id,),
                            eth_account,
                            contract::Options::default(),
                            None,
                        ).map_err(|e| error!("readDataRequest {:?}", e)))
                }
                _ => {
                    debug!("[{}] is not claimable", dr_id);

                    Either::B(futures::failed(()))
                }
            }


        })
        .and_then(move |dr_bytes: Bytes| {
            let eth_state2 = eth_state.clone();
            let ignore_dr = move |dr_id| {
                eth_state2.wrb_requests.write().and_then(move |mut wrb_requests| {
                    wrb_requests.ignore(dr_id);

                    futures::finished(())
                }).then(|_| {
                    futures::failed(())
                })
            };
            let dr_output: DataRequestOutput =
                match ProtobufConvert::from_pb_bytes(&dr_bytes).and_then(|dr: DataRequestOutput| {
                    validate_rad_request(&dr.data_request)?;
                    Ok(dr)
                }) {
                    Ok(x) => {
                        debug!("{:?}", x);
                        // TODO: check if we want to claim this data request:
                        // Is the price ok?

                        // Is the data request serialized correctly?
                        // Check that serializing the deserialized struct results in exactly the same bytes
                        let witnet_dr_bytes = x.to_pb_bytes();

                        match witnet_dr_bytes {
                            Ok(witnet_dr_bytes) => if dr_bytes == witnet_dr_bytes {
                                x
                            } else {
                                warn!(
                                    "[{}] uses an invalid serialization, will be ignored.\nETH DR BYTES: {:02x?}\nWIT DR BYTES: {:02x?}",
                                    dr_id, dr_bytes, witnet_dr_bytes
                                );
                                warn!("This usually happens when some fields are set to 0. \
                                       The Rust implementation of ProtocolBuffer skips those fields, \
                                       as missing fields are deserialized with the default value.");
                                return Either::B(ignore_dr(dr_id));
                            },
                            Err(e) => {
                                warn!(
                                    "[{}] uses an invalid serialization, will be ignored: {:?}",
                                    dr_id, e
                                );
                                return Either::B(ignore_dr(dr_id));
                            }
                        }
                    },
                    Err(e) => {
                        warn!(
                            "[{}] uses an invalid serialization, will be ignored: {:?}",
                            dr_id, e
                        );
                        return Either::B(ignore_dr(dr_id));
                    }
                };

            Either::A(
                wrb_contract2
                    .query(
                        "getLastBeacon",
                        (),
                        eth_account,
                        contract::Options::default(),
                        None,
                    )
                    .map(|x: Bytes| (x, dr_output))
                    .map_err(|e| error!("getLastBeacon {:?}", e)),
            )
        })
        .and_then(move |(vrf_message, dr_output)| {
            let last_beacon = vrf_message.clone();

            witnet_client2
                .execute("createVRF", vrf_message.into())
                .map_err(|e| error!("createVRF: {:?}", e))
                .map(move |vrf| {
                    trace!("createVRF: {:?}", vrf);

                    (vrf, dr_output, last_beacon)
                })
        })
        .and_then(move |(vrf, dr_output, last_beacon)| {
            // Sign the ethereum account address with the witnet node private key
            let Sha256(hash) = calculate_sha256(eth_account.as_bytes());

            witnet_client3
                .execute("sign", hash.to_vec().into())
                .map_err(|e| error!("sign: {:?}", e))
                .map(|sign_addr| {
                    trace!("sign: {:?}", sign_addr);

                    (vrf, sign_addr, dr_output, last_beacon)
                })
        })
        .and_then(move |(vrf, sign_addr, dr_output, last_beacon)| {
            // Get the public key of the witnet node

            witnet_client4
                .execute("getPublicKey", json!(null))
                .map_err(|e| error!("getPublicKey: {:?}", e))
                .map(move |witnet_pk| {
                    trace!("getPublicKey: {:?}", witnet_pk);

                    (vrf, sign_addr, witnet_pk, dr_output, last_beacon)
                })
        })
        .and_then(move |(vrf, sign_addr, witnet_pk, dr_output, last_beacon)| {

            // Locally execute POE verification to check for eligibility
            // without spending any gas
            let poe = convert_json_array_to_eth_bytes(vrf);
            let witnet_pk = convert_json_array_to_eth_bytes(witnet_pk);
            let sign_addr = convert_json_array_to_eth_bytes(sign_addr);

            let (poe, witnet_pk, sign_addr) = match (poe, witnet_pk, sign_addr) {
                (Ok(poe), Ok(witnet_pk), Ok(sign_addr)) => {
                    (poe, witnet_pk, sign_addr)
                }
                e => {
                    error!(
                        "Error deserializing value from witnet JSONRPC: {:?}",
                        e
                    );
                    let fut: Box<
                        dyn Future<Item = (_, _, _, _, _), Error = ()> + Send,
                    > = Box::new(futures::failed(()));
                    return fut;
                }
            };

            debug!(
                "\nPoE: {:?}\nWitnet Public Key: {:?}\nSignature Address: {:?}",
                poe, witnet_pk.clone(), sign_addr
            );
            info!("[{}] Checking eligibility for claiming dr", dr_id);

            Box::new(
                wrb_contract5
                    .query(
                        "decodePoint",
                        witnet_pk,
                        eth_account,
                        contract::Options::default(),
                        None,
                    )
                    .map_err(move |e| {
                        warn!(
                            "[{}] Error decoding public Key:  {}",
                            dr_id, e);
                    })
                    .map(move |pk: [U256; 2]| {
                        debug!("Received public key decode Point: {:?}", pk);

                        (poe, sign_addr, pk, dr_output, last_beacon)
                    }),
            )
        })
        .and_then(move |(poe, sign_addr, witnet_pk, dr_output, last_beacon)| {

            Box::new(
                wrb_contract6
                    .query(
                        "decodeProof",
                        poe,
                        eth_account,
                        contract::Options::default(),
                        None,
                    )
                    .map_err(move |e| {
                        warn!(
                            "[{}] Error decoding proof:  {}",
                            dr_id, e);
                    })
                    .map(move |proof: [U256; 4]| {
                        debug!("Received proof decode Point: {:?}", proof);

                        (proof, sign_addr, witnet_pk, dr_output, last_beacon)
                    }),
            )
        })
        .and_then(move |(poe, sign_addr, witnet_pk, dr_output, last_beacon)| {

            Box::new(
                wrb_contract7
                    .query(
                        "computeFastVerifyParams",
                        (witnet_pk, poe, last_beacon),
                        eth_account,
                        contract::Options::default(),
                        None,
                    )
                    .map_err(move |e| {
                        warn!(
                            "[{}] Error in params reception:  {}",
                            dr_id, e);
                    })
                    .map(move |(u_point, v_point): ([U256; 2], [U256; 4])| {
                        debug!("Received fast verify params: ({:?}, {:?})", u_point, v_point);

                        (poe, sign_addr, witnet_pk, dr_output, u_point , v_point)
                    }),
            )
        })
        .and_then(move |(poe, sign_addr, witnet_pk, dr_output, u_point , v_point)| {
            let mut sign_addr2 = sign_addr.clone();
            // Append v value to the signature, as it is needed by Ethereum but
            // it is not provided by OpenSSL. Fortunately, it is only 1 bit so
            // we can bruteforce the v value by setting it to 0, and if it
            // fails, setting it to 1.
            sign_addr2.push(0);
            let fut1 = wrb_contract3
                .query(
                    "claimDataRequests",
                    (vec![dr_id], poe, witnet_pk, u_point, v_point, sign_addr.clone()),
                    eth_account,
                    contract::Options::default(),
                    None,
                )
                .map(|_: Token| sign_addr);
            // If the query fails, we want to retry it with the signature "v" value flipped.
            *sign_addr2.last_mut().unwrap() ^= 0x01;
            let fut2 = wrb_contract3
                .query(
                    "claimDataRequests",
                    (vec![dr_id], poe, witnet_pk, u_point, v_point, sign_addr2.clone()),
                    eth_account,
                    contract::Options::default(),
                    None,
                )
                .map(|_: Token| sign_addr2);

            Box::new(
                fut1
                    .or_else(move |e| {
                        debug!("claimDataRequests failed, retrying with different signature sign (v): {:?}", e);
                        Box::new(fut2)
                    })
                    .map_err(move |e| {
                        warn!(
                            "[{}] the POE is invalid, no eligibility for this epoch, or the data request has already been claimed :( {:?}",
                            dr_id, e);
                    })
                    .map(move |sign_addr| {
                        (poe, sign_addr, witnet_pk, dr_output, u_point, v_point)
                    }),
            )

        })
        .map(move |(poe, sign_addr, witnet_pk, dr_output, u_point, v_point)| {
            (dr_output, (vec![dr_id], poe, witnet_pk, u_point, v_point, sign_addr))
        })
}

/// Try to claim DR in WRB and post it to Witnet
fn claim_and_post_dr(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
    witnet_client: Arc<TcpSocket>,
    dr_id: U256,
) -> impl Future<Item = (), Error = ()> {
    let config2 = config.clone();
    let eth_account = config.eth_account;
    let post_to_witnet_more_than_once = config.post_to_witnet_more_than_once;

    let wrb_contract = eth_state.wrb_contract.clone();
    let witnet_client = Arc::clone(&witnet_client);

    try_to_claim_local_query(config, Arc::clone(&eth_state), Arc::clone(&witnet_client), dr_id)
        .and_then(move |(dr_output, claim_data_requests_params)| {
            // Claim dr
            info!("[{}] Claiming dr", dr_id);
            let dr_output_hash = dr_output.hash();
            let dr_output = Arc::new(dr_output);
            let dr_output2 = Arc::clone(&dr_output);
            let witnet_client2 = witnet_client.clone();

            // Mark the data request as claimed to prevent double claims by other threads
            eth_state.wrb_requests.write()
                .and_then(move |mut wrb_requests| {
                    if wrb_requests.posted().contains(&dr_id) {
                        wrb_requests.set_claiming(dr_id);
                        Either::A(futures::finished(()))
                    } else if post_to_witnet_more_than_once && wrb_requests.claimed().contains_left(&dr_id) {
                        // Post dr in witnet again.
                        // This may lead to double spending wits.
                        // This can be useful in the following scenarios:
                        // * The data request is posted to Witnet, but it
                        //   is not accepted into a Witnet block
                        //   (or is invalid because of double-spending).

                        warn!("[{}] Posting to witnet again as we have not received a block containing this data request yet", dr_id);

                        let bdr_params = json!({"dro": dr_output2, "fee": 0});

                        Either::B(witnet_client2
                            .execute("sendRequest", bdr_params)
                            .map_err(|e| error!("sendRequest: {:?}", e))
                            .map(move |bdr_res| {
                                debug!("sendRequest: {:?}", bdr_res);
                            }).then(|_| futures::failed(())))
                    } else {
                        // This data request is not available, abort.
                        debug!("[{}] is not available for claiming, skipping", dr_id);
                        Either::A(futures::failed(()))
                    }
                })
                .and_then(move |()| {
                    let eth_state2 = eth_state.clone();

                    wrb_contract
                        .call_with_confirmations(
                            "claimDataRequests",
                            claim_data_requests_params,
                            eth_account,
                            contract::Options::with(|opt| {
                                opt.gas = config2.gas_limits.claim_data_requests.map(Into::into);
                            }),
                            1,
                        )
                        .map_err(|e| {
                            error!("claimDataRequests: {:?}", e);
                        })
                        .and_then(move |tx| {
                            debug!("claimDataRequests: {:?}", tx);
                            handle_receipt(tx).map_err(move |_| {
                                // Or the PoE became invalid because a new witnet block was
                                // just relayed
                                // In this case we should save this data request to retry later
                                warn!(
                                    "[{}] has been claimed by another bridge node, or the PoE expired",
                                    dr_id
                                );
                            })
                        })
                        .and_then(move |()| {
                            eth_state.wrb_requests.write().map(move |mut wrb_requests| {
                                wrb_requests.confirm_claim(dr_id, dr_output_hash);
                            })
                        })
                        .or_else(move |()| {
                            // Undo the claim
                            eth_state2.wrb_requests.write().map(move |mut wrb_requests| {
                                wrb_requests.undo_claim(dr_id);
                            }).then(|_| {
                                // Short-circuit the and_then cascade
                                Err(())
                            })
                        })
                })
                .and_then(move |_traces| {
                    // Post dr in witnet
                    info!("[{}] Claimed dr, posting to witnet", dr_id);

                    let bdr_params = json!({"dro": dr_output, "fee": 0});

                    witnet_client
                        .execute("sendRequest", bdr_params)
                        .map_err(|e| error!("sendRequest: {:?}", e))
                        .map(move |bdr_res| {
                            debug!("sendRequest: {:?}", bdr_res);
                        })
                })
                .map(|_| ())
        })
}

/// Actor which tries to claim data requests from WRB and posts them to Witnet
pub fn claim_and_post(
    config: Arc<Config>,
    eth_state: Arc<EthState>,
) -> (
    async_jsonrpc_client::transports::shared::EventLoopHandle,
    mpsc::Sender<ClaimMsg>,
    impl Future<Item = (), Error = ()>,
) {
    // Important: the handle cannot be dropped, otherwise the client stops
    // processing events
    let witnet_addr = config.witnet_jsonrpc_addr.to_string();
    let (handle, witnet_client) =
        async_jsonrpc_client::transports::tcp::TcpSocket::new(&witnet_addr).unwrap();
    let witnet_client = Arc::new(witnet_client);
    let (tx, rx) = mpsc::channel(16);

    (
        handle,
        tx,
        rx.map_err(|_| ()).for_each(move |msg| {
            if !config.enable_claim_and_inclusion {
                return futures::finished(());
            }
            debug!("Got PostActorMessage: {:?}", msg);

            let config2 = Arc::clone(&config);
            let eth_state2 = Arc::clone(&eth_state);
            let witnet_client2 = Arc::clone(&witnet_client);

            let fut = match msg {
                ClaimMsg::NewDr(dr_id) => Either::A(claim_and_post_dr(
                    config.clone(),
                    eth_state.clone(),
                    witnet_client.clone(),
                    dr_id,
                )),
                ClaimMsg::Tick => {
                    Either::B(eth_state.wrb_requests.read().and_then(move |known_dr_ids| {
                        let known_dr_ids_posted = known_dr_ids.posted();
                        let known_dr_ids_claimed = known_dr_ids.claimed();
                        let sorted_dr_state: BTreeMap<_, _> =
                            known_dr_ids.requests().iter().collect();
                        debug!("{:?}", sorted_dr_state);
                        debug!(
                            "Known data requests in WRB: {:?}{:?}",
                            known_dr_ids_posted, known_dr_ids_claimed
                        );

                        // Chose a random data request and try to claim and post it.
                        // Gives preference to newly posted data requests
                        match (
                            known_dr_ids_posted.is_empty(),
                            known_dr_ids_claimed.is_empty(),
                        ) {
                            (true, true) => Either::B(futures::finished(())),
                            (false, _) => {
                                let i = thread_rng().gen_range(0, known_dr_ids_posted.len());
                                let dr_id = *known_dr_ids_posted.iter().nth(i).unwrap();
                                std::mem::drop(known_dr_ids);

                                Either::A(claim_and_post_dr(
                                    config2.clone(),
                                    eth_state2.clone(),
                                    witnet_client2,
                                    dr_id,
                                ))
                            }
                            _ => {
                                // Try to claim already-claimed data request as the claim may
                                // have expired.
                                let i = thread_rng().gen_range(0, known_dr_ids_claimed.len());
                                let dr_id = *known_dr_ids_claimed.iter().nth(i).unwrap().0;
                                std::mem::drop(known_dr_ids);

                                Either::A(claim_and_post_dr(
                                    config2.clone(),
                                    eth_state2.clone(),
                                    witnet_client2,
                                    dr_id,
                                ))
                            }
                        }
                    }))
                }
            };

            // Start the claim as a separate task, to avoid blocking this receiver
            tokio::spawn(fut);

            futures::finished(())
        }),
    )
}

/// Periodically try to claim a random data request
pub fn claim_ticker(
    config: Arc<Config>,
    post_tx: mpsc::Sender<ClaimMsg>,
) -> impl Future<Item = (), Error = ()> {
    Interval::new(
        Instant::now(),
        Duration::from_millis(config.claim_dr_rate_ms),
    )
    .map_err(|e| error!("Error creating interval: {:?}", e))
    .and_then(move |_instant| {
        post_tx
            .clone()
            .send(ClaimMsg::Tick)
            .map_err(|e| error!("Error sending tick to PostActor: {:?}", e))
    })
    .for_each(|_| Ok(()))
}
