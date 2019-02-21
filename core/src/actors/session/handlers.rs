use std::io::Error;

use actix::io::WriteHandler;
use actix::{
    ActorContext, ActorFuture, Context, ContextFutureSpawner, Handler, StreamHandler, System,
    SystemService, WrapFuture,
};
use ansi_term::Color::Green;
use futures::future;
use log::{debug, error, info, trace, warn};

use witnet_data_structures::{
    builders::from_address,
    chain::{Block, CheckpointBeacon, InventoryEntry, InventoryItem, Transaction},
    proto::ProtobufConvert,
    types::{
        Address, Command, InventoryAnnouncement, InventoryRequest, LastBeacon,
        Message as WitnetMessage, Peers, Version,
    },
};
use witnet_p2p::sessions::{SessionStatus, SessionType};

use crate::actors::{
    chain_manager::{
        messages::{
            AddNewBlock, AddTransaction, DiscardExistingInventoryEntries, GetBlocksEpochRange,
            GetHighestCheckpointBeacon, PeerLastEpoch,
        },
        ChainManager,
    },
    codec::BytesMut,
    inventory_manager::{messages::GetItem, InventoryManager},
    peers_manager,
    sessions_manager::{messages::Consolidate, SessionsManager},
};

use super::{
    messages::{
        AnnounceItems, GetPeers, InventoryExchange, RequestBlock, SendInventoryItem,
        SessionUnitResult,
    },
    Session,
};

/// Implement WriteHandler for Session
impl WriteHandler<Error> for Session {}

/// Implement `StreamHandler` trait in order to use `Framed` with an actor
impl StreamHandler<BytesMut, Error> for Session {
    /// This is main event loop for client requests
    fn handle(&mut self, bytes: BytesMut, ctx: &mut Self::Context) {
        let result = WitnetMessage::from_pb_bytes(&bytes);
        match result {
            Err(err) => error!("Error decoding message: {:?}", err),
            Ok(msg) => {
                debug!(
                    "{} Received {} message from session {:?}",
                    Green.bold().paint("[<]"),
                    Green.bold().paint(msg.kind.to_string()),
                    self.remote_addr,
                );
                trace!("\t{:?}", msg);
                match (self.session_type, self.status, msg.kind) {
                    ////////////////////
                    //   HANDSHAKE    //
                    ////////////////////
                    // Handle Version message
                    (
                        _,
                        SessionStatus::Unconsolidated,
                        Command::Version(Version { sender_address, .. }),
                    ) => {
                        let msgs = handshake_version(self, &sender_address);
                        for msg in msgs {
                            self.send_message(msg);
                        }
                        try_consolidate_session(self, ctx);
                    }
                    // Handler Verack message
                    (_, SessionStatus::Unconsolidated, Command::Verack(_)) => {
                        handshake_verack(self);
                        try_consolidate_session(self, ctx);
                    }
                    ////////////////////
                    // PEER DISCOVERY //
                    ////////////////////
                    // Handle GetPeers message
                    (_, SessionStatus::Consolidated, Command::GetPeers(_)) => {
                        peer_discovery_get_peers(self, ctx);
                    }
                    // Handle Peers message
                    (_, SessionStatus::Consolidated, Command::Peers(Peers { peers })) => {
                        peer_discovery_peers(&peers);
                    }
                    ///////////////////////
                    // INVENTORY_REQUEST //
                    ///////////////////////
                    (
                        _,
                        SessionStatus::Consolidated,
                        Command::InventoryRequest(InventoryRequest { inventory }),
                    ) => {
                        let inventory_mngr = System::current().registry().get::<InventoryManager>();
                        let item_requests: Vec<_> = inventory
                            .iter()
                            .filter_map(|item| match item {
                                InventoryEntry::Block(hash) | InventoryEntry::Tx(hash) => {
                                    Some(inventory_mngr.send(GetItem { hash: *hash }))
                                }
                                _ => None,
                            })
                            .collect();

                        future::join_all(item_requests)
                            .into_actor(self)
                            .map_err(|e, _, _| error!("Inventory request error: {}", e))
                            .and_then(|item_responses, session, _| {
                                for item_response in item_responses {
                                    match item_response {
                                        Ok(item) => send_inventory_item_msg(session, item),
                                        Err(e) => warn!("Inventory result is error: {}", e),
                                    }
                                }

                                actix::fut::ok(())
                            })
                            .spawn(ctx);
                    }
                    //////////////////////////
                    // TRANSACTION RECEIVED //
                    //////////////////////////
                    (_, SessionStatus::Consolidated, Command::Transaction(transaction)) => {
                        inventory_process_transaction(self, ctx, transaction);
                    }

                    ////////////////////
                    // BLOCK RECEIVED //
                    ////////////////////
                    // Handle Block
                    (_, SessionStatus::Consolidated, Command::Block(block)) => {
                        inventory_process_block(self, ctx, block);
                    }

                    /////////////////
                    // LAST BEACON //
                    /////////////////
                    (
                        SessionType::Inbound,
                        SessionStatus::Consolidated,
                        Command::LastBeacon(LastBeacon {
                            highest_block_checkpoint,
                        }),
                    ) => {
                        session_last_beacon_inbound(self, ctx, highest_block_checkpoint);
                    }
                    (
                        SessionType::Outbound,
                        SessionStatus::Consolidated,
                        Command::LastBeacon(LastBeacon {
                            highest_block_checkpoint,
                        }),
                    ) => {
                        session_last_beacon_outbound(self, ctx, highest_block_checkpoint);
                    }

                    ////////////////////
                    // INVENTORY      //
                    ////////////////////
                    // Handle InventoryAnnouncement message
                    (_, SessionStatus::Consolidated, Command::InventoryAnnouncement(inv)) => {
                        inventory_process_inv(self, ctx, &inv);
                    }
                    /////////////////////
                    // NOT SUPPORTED   //
                    /////////////////////
                    (session_type, session_status, msg_type) => {
                        warn!(
                            "Message of type \"{:?}\" for session (type: {:?}, status: {:?}) is \
                             not supported",
                            msg_type, session_type, session_status
                        );
                    }
                };
            }
        }
    }
}

/// Handler for GetPeers message (sent by other actors)
impl Handler<GetPeers> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, _msg: GetPeers, _: &mut Context<Self>) {
        debug!("Sending GetPeers message to peer at {:?}", self.remote_addr);
        // Create get peers message
        let get_peers_msg = WitnetMessage::build_get_peers(self.magic_number);
        // Write get peers message in session
        self.send_message(get_peers_msg);
    }
}

/// Handler for AnnounceItems message (sent by other actors)
impl Handler<AnnounceItems> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: AnnounceItems, _: &mut Context<Self>) {
        debug!(
            "Sending AnnounceItems message to peer at {:?}",
            self.remote_addr
        );
        // Try to create AnnounceItems message with items to be announced
        if let Ok(announce_items_msg) =
            WitnetMessage::build_inventory_announcement(self.magic_number, msg.items)
        {
            // Send message through the session network connection
            self.send_message(announce_items_msg);
        };
    }
}

/// Handler for SendInventoryItem message (sent by other actors)
impl Handler<SendInventoryItem> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: SendInventoryItem, _: &mut Context<Self>) {
        debug!(
            "Sending SendInventoryItem message to peer at {:?}",
            self.remote_addr
        );
        send_inventory_item_msg(self, msg.item)
    }
}

/// Handler for RequestBlock message (sent by other actors)
impl Handler<RequestBlock> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: RequestBlock, _: &mut Context<Self>) {
        debug!(
            "Sending RequestBlock message to peer at {:?}",
            self.remote_addr
        );
        request_block_msg(self, msg.block_entry)
    }
}

/// Handler for InventoryExchange message (sent by other actors)
impl Handler<InventoryExchange> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, _: InventoryExchange, ctx: &mut Context<Self>) {
        debug!(
            "Starting Inventory Exchange with peer at {:?}",
            self.remote_addr
        );
        inventory_get_blocks(self, ctx)
    }
}

/// Function to try to consolidate session if handshake conditions are met
fn try_consolidate_session(session: &mut Session, ctx: &mut Context<Session>) {
    // Check if HandshakeFlags are all set to true
    if session.handshake_flags.all_true() && session.remote_sender_addr.is_some() {
        // Update session to consolidate status
        update_consolidate(session, ctx);
    }
}

/// Function to retrieve highest CheckpointBeacon and send LastBeacon message in Session
fn inventory_get_blocks(session: &Session, ctx: &mut Context<Session>) {
    // Get ChainManager address from registry
    let chain_manager_addr = System::current().registry().get::<ChainManager>();
    // Send GetHighestCheckpointBeacon message to ChainManager
    chain_manager_addr
        .send(GetHighestCheckpointBeacon)
        .into_actor(session)
        .then(|res, act, ctx| {
            match res {
                Ok(Ok(beacon)) => {
                    // Create get blocks message
                    let get_blocks_msg = WitnetMessage::build_last_beacon(act.magic_number, beacon);
                    // Write get blocks message in session
                    act.send_message(get_blocks_msg);

                    actix::fut::ok(())
                }
                _ => {
                    warn!("Failed to get highest checkpoint beacon from ChainManager");
                    // FIXME(#72): a full stop of the session is not correct (unregister should
                    // be skipped)
                    ctx.stop();

                    actix::fut::err(())
                }
            }
        })
        .wait(ctx);
}

// Function to notify the SessionsManager that the session has been consolidated
fn update_consolidate(session: &Session, ctx: &mut Context<Session>) {
    // Get session manager address
    let session_manager_addr = System::current().registry().get::<SessionsManager>();

    // Register self in session manager. `AsyncContext::wait` register
    // future within context, but context waits until this future resolves
    // before processing any other events.
    session_manager_addr
        .send(Consolidate {
            address: session.remote_addr,
            potential_new_peer: session.remote_sender_addr.unwrap(),
            session_type: session.session_type,
        })
        .into_actor(session)
        .then(|res, act, ctx| {
            match res {
                Ok(Ok(_)) => {
                    debug!(
                        "Successfully consolidated session {:?} in SessionManager",
                        act.remote_addr
                    );
                    // Set status to consolidate
                    act.status = SessionStatus::Consolidated;

                    actix::fut::ok(())
                }
                _ => {
                    warn!(
                        "Failed to consolidate session {:?} in SessionManager",
                        act.remote_addr
                    );
                    // FIXME(#72): a full stop of the session is not correct (unregister should
                    // be skipped)
                    ctx.stop();

                    actix::fut::err(())
                }
            }
        })
        .wait(ctx);
}

/// Function called when GetPeers message is received
fn peer_discovery_get_peers(session: &mut Session, ctx: &mut Context<Session>) {
    // Get the address of PeersManager actor
    let peers_manager_addr = System::current()
        .registry()
        .get::<peers_manager::PeersManager>();

    // Start chain of actions
    peers_manager_addr
        // Send GetPeer message to PeersManager actor
        // This returns a Request Future, representing an asynchronous message sending process
        .send(peers_manager::messages::GetPeers)
        // Convert a normal future into an ActorFuture
        .into_actor(session)
        // Process the response from PeersManager
        // This returns a FutureResult containing the socket address if present
        .then(|res, act, ctx| {
            match res {
                Ok(Ok(addresses)) => {
                    debug!(
                        "Received {} peer addresses from PeersManager",
                        addresses.len()
                    );
                    let peers_msg = WitnetMessage::build_peers(act.magic_number, &addresses);
                    act.send_message(peers_msg);
                }
                _ => {
                    warn!("Failed to receive peer addresses from PeersManager");
                    // FIXME(#72): a full stop of the session is not correct (unregister should
                    // be skipped)
                    ctx.stop();
                }
            }
            actix::fut::ok(())
        })
        .wait(ctx);
}

/// Function called when Peers message is received
fn peer_discovery_peers(peers: &[Address]) {
    // Get peers manager address
    let peers_manager_addr = System::current()
        .registry()
        .get::<peers_manager::PeersManager>();

    // Convert array of address to vector of socket addresses
    let addresses = peers.iter().map(from_address).collect();

    // Send AddPeers message to the peers manager
    peers_manager_addr.do_send(peers_manager::messages::AddPeers {
        // TODO: convert Vec<Address> to Vec<SocketAddr>
        addresses,
    });
}

/// Function called when Block message is received
fn inventory_process_block(_session: &mut Session, _ctx: &mut Context<Session>, block: Block) {
    // Get ChainManager address
    let chain_manager_addr = System::current().registry().get::<ChainManager>();

    // Send a message to the ChainManager to try to add a new block
    chain_manager_addr.do_send(AddNewBlock { block });
}

/// Function called when Block message is received
fn inventory_process_transaction(
    _session: &mut Session,
    _ctx: &mut Context<Session>,
    transaction: Transaction,
) {
    // Get ChainManager address
    let chain_manager_addr = System::current().registry().get::<ChainManager>();

    // Send a message to the ChainManager to try to add a new transaction
    chain_manager_addr.do_send(AddTransaction { transaction });
}

/// Function to process an InventoryAnnouncement message
fn inventory_process_inv(
    session: &mut Session,
    ctx: &mut Context<Session>,
    inv: &InventoryAnnouncement,
) {
    // Check how many of the received inventory vectors need to be requested
    let inv_entries = &inv.inventory;

    // Get ChainManager address
    let chain_manager_addr = System::current().registry().get::<ChainManager>();

    // Send a message to the ChainManager to try to add a new block
    chain_manager_addr
        // Send GetConfig message to config manager actor
        // This returns a Request Future, representing an asynchronous message sending process
        .send(DiscardExistingInventoryEntries {
            inv_entries: inv_entries.to_vec(),
        })
        // Convert a normal future into an ActorFuture
        .into_actor(session)
        // Process the response from the Chain Manager
        // This returns a FutureResult containing the socket address if present
        .then(|res, _act, _ctx| {
            // Process the Result<InventoryEntriesResult, MailboxError>
            match res {
                Err(e) => {
                    error!("Unsuccessful communication with Chain Manager: {}", e);
                    actix::fut::err(())
                }
                Ok(res) => match res {
                    Err(_) => {
                        error!("Error while filtering inventory vectors");
                        actix::fut::err(())
                    }
                    Ok(res) => actix::fut::ok(res),
                },
            }
        })
        // Process the received filtered inv elems
        // This returns a FutureResult containing a success
        .and_then(|missing_inv_entries, act, _ctx| {
            // Try to create InventoryRequest protocol message to request missing inventory vectors
            if let Ok(inv_req_msg) = WitnetMessage::build_inventory_request(
                act.magic_number,
                missing_inv_entries.to_vec(),
            ) {
                // Send InventoryRequest message through the session network connection
                act.send_message(inv_req_msg);
            }

            actix::fut::ok(())
        })
        .wait(ctx);
}

/// Function called when Verack message is received
fn handshake_verack(session: &mut Session) {
    let flags = &mut session.handshake_flags;

    if flags.verack_rx {
        debug!("Verack message already received");
    }

    // Set verack_rx flag
    flags.verack_rx = true;
}

/// Function called when Version message is received
fn handshake_version(session: &mut Session, sender_address: &Address) -> Vec<WitnetMessage> {
    let flags = &mut session.handshake_flags;

    if flags.version_rx {
        debug!("Version message already received");
    }

    // Placeholder for version fields verification
    session.remote_sender_addr = Some(from_address(sender_address));

    // Set version_rx flag, indicating reception of a version message from the peer
    flags.version_rx = true;

    let mut responses: Vec<WitnetMessage> = vec![];
    if !flags.verack_tx {
        flags.verack_tx = true;
        let verack = WitnetMessage::build_verack(session.magic_number);
        responses.push(verack);
    }
    if !flags.version_tx {
        flags.version_tx = true;
        let version = WitnetMessage::build_version(
            session.magic_number,
            session.server_addr,
            session.remote_addr,
            0,
        );
        responses.push(version);
    }

    responses
}

fn send_inventory_item_msg(session: &mut Session, item: InventoryItem) {
    match item {
        InventoryItem::Block(block) => {
            let block_header = block.block_header;
            let proof = block.proof;
            let txns = block.txns;
            // Build Block msg
            let block_msg =
                WitnetMessage::build_block(session.magic_number, block_header, proof, txns);
            // Send Block msg
            session.send_message(block_msg);
        }
        InventoryItem::Transaction(transaction) => {
            // Build Transaction msg
            let transaction_msg =
                WitnetMessage::build_transaction(session.magic_number, transaction);
            // Send Transaction msg
            session.send_message(transaction_msg);
        }
    }
}

fn request_block_msg(session: &mut Session, block_entry: InventoryEntry) {
    // Initialize a new inventory entries vector with the given block entry as its sole member
    let inv_entries: Vec<InventoryEntry> = vec![block_entry];

    match WitnetMessage::build_inventory_request(session.magic_number, inv_entries) {
        Ok(inv_req_msg) => {
            // Send InventoryRequest message through the session network connection
            session.send_message(inv_req_msg);
        }
        Err(e) => {
            // BuildersErrorKind error
            warn!("Error creating and inventory request message: {}", e);
        }
    }
}

fn session_last_beacon_inbound(
    session: &Session,
    ctx: &mut Context<Session>,
    CheckpointBeacon {
        checkpoint: received_checkpoint,
        ..
    }: CheckpointBeacon,
) {
    // Get ChainManager address from registry
    let chain_manager_addr = System::current().registry().get::<ChainManager>();
    // Send GetHighestCheckpointBeacon message to ChainManager
    chain_manager_addr
        .send(GetHighestCheckpointBeacon)
        .into_actor(session)
        .then(move |res, act, ctx| {
            match res {
                Ok(Ok(chain_beacon)) => {
                    if chain_beacon.checkpoint > received_checkpoint {
                        let range = (received_checkpoint + 1)..=chain_beacon.checkpoint;

                        chain_manager_addr
                            .send(GetBlocksEpochRange::new_with_const_limit(range))
                            .into_actor(act)
                            .then(|res, act, _ctx| match res {
                                Ok(Ok(blocks)) => {
                                    // Try to create an Inv protocol message with the items to
                                    // be announced
                                    if let Ok(inv_msg) =
                                        WitnetMessage::build_inventory_announcement(act.magic_number, blocks.into_iter().map(|(_epoch, hash)| hash).collect())
                                    {
                                        // Send Inv message through the session network connection
                                        act.send_message(inv_msg);
                                    };

                                    actix::fut::ok(())
                                }
                                _ => {
                                    error!("LastBeacon::EpochRange didn't succeeded");

                                    actix::fut::err(())
                                }
                            })
                            .wait(ctx);
                    } else if chain_beacon.checkpoint == received_checkpoint {
                        info!("Our chain is on par with our peer's",);
                        // Create a last_beacon message
                        let last_beacon_msg = WitnetMessage::build_last_beacon(act.magic_number, chain_beacon);
                        // Write a last_beacon message in session
                        act.send_message(last_beacon_msg);
                    } else {
                        warn!(
                            "Received a checkpoint beacon that is ahead of ours ({} > {})",
                            received_checkpoint, chain_beacon.checkpoint
                        );
                    }

                    actix::fut::ok(())
                }
                _ => {
                    warn!("Failed to get highest checkpoint beacon from ChainManager");
                    // FIXME(#72): a full stop of the session is not correct (unregister should
                    // be skipped)
                    ctx.stop();

                    actix::fut::err(())
                }
            }
        })
        .wait(ctx);
}

fn session_last_beacon_outbound(
    session: &Session,
    ctx: &mut Context<Session>,
    CheckpointBeacon {
        checkpoint: received_checkpoint,
        ..
    }: CheckpointBeacon,
) {
    // Get ChainManager address from registry
    let chain_manager_addr = System::current().registry().get::<ChainManager>();
    // Send GetHighestCheckpointBeacon message to ChainManager
    chain_manager_addr
        .send(GetHighestCheckpointBeacon)
        .into_actor(session)
        .then(move |res, _act, ctx| {
            match res {
                Ok(Ok(chain_beacon)) => {
                    if chain_beacon.checkpoint > received_checkpoint {
                        warn!(
                            "My outbound peer is behind me (Outbound checkpoint:{} < Chain checkpoint:{})",
                            received_checkpoint, chain_beacon.checkpoint
                        );
                    } else if chain_beacon.checkpoint == received_checkpoint {
                        info!("Our chain is on par with our peer's",);
                    } else {
                        debug!(
                            "Received a checkpoint beacon that is ahead of ours ({} > {})",
                            received_checkpoint, chain_beacon.checkpoint
                        );
                    }

                    // Handle mine boolean
                    ChainManager::from_registry().do_send(PeerLastEpoch {
                        epoch: received_checkpoint,
                    });

                    actix::fut::ok(())
                }
                _ => {
                    warn!("Failed to get highest checkpoint beacon from ChainManager");
                    // FIXME(#72): a full stop of the session is not correct (unregister should
                    // be skipped)
                    ctx.stop();

                    actix::fut::err(())
                }
            }
        })
        .wait(ctx);
}
