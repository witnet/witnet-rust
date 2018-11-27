use std::io::Error;

use actix::io::WriteHandler;
use actix::{
    ActorContext, ActorFuture, Context, ContextFutureSpawner, Handler, StreamHandler, System,
    WrapFuture,
};

use log::{debug, error, info, warn};

use crate::actors::{
    blocks_manager::{
        messages::{AddNewBlock, GetHighestCheckpointBeacon},
        BlocksManager,
    },
    codec::BytesMut,
    peers_manager,
    sessions_manager::{messages::Consolidate, SessionsManager},
    storage_manager::{messages::Get, StorageManager},
};

use super::{
    messages::{AnnounceItems, GetPeers, SessionUnitResult},
    Session,
};
use witnet_data_structures::{
    builders::from_address,
    chain::{Block, Hash, InvVector},
    serializers::TryFrom,
    types::{Address, Command, GetData, Inv, Message as WitnetMessage, Peers, Version},
};
use witnet_p2p::sessions::{SessionStatus, SessionType};

/// Implement WriteHandler for Session
impl WriteHandler<Error> for Session {}

/// Implement `StreamHandler` trait in order to use `Framed` with an actor
impl StreamHandler<BytesMut, Error> for Session {
    /// This is main event loop for client requests
    fn handle(&mut self, bytes: BytesMut, ctx: &mut Self::Context) {
        let result = WitnetMessage::try_from(bytes.to_vec());
        match result {
            Err(err) => error!("Error decoding message: {:?}", err),
            Ok(msg) => {
                info!(
                    "<----- Session ({}) received message: {}",
                    self.remote_addr, msg.kind
                );
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
                    (
                        SessionType::Outbound,
                        SessionStatus::Consolidated,
                        Command::Peers(Peers { peers }),
                    ) => {
                        peer_discovery_peers(&peers);
                    }
                    //////////////
                    // GET DATA //
                    //////////////
                    (_, SessionStatus::Consolidated, Command::GetData(GetData { inventory })) => {
                        for elem in inventory {
                            match elem {
                                InvVector::Block(hash)
                                | InvVector::Tx(hash)
                                | InvVector::DataRequest(hash)
                                | InvVector::DataResult(hash) => {
                                    send_block_msg(self, ctx, &hash);
                                }
                                InvVector::Error(_) => warn!("Error InvElem received"),
                            }
                        }
                    }
                    ////////////////////
                    // BLOCK RECEIVED //
                    ////////////////////
                    // Handle Block
                    (_, SessionStatus::Consolidated, Command::Block(block)) => {
                        inventory_process_block(self, ctx, block);
                    }
                    ////////////////////
                    // INVENTORY      //
                    ////////////////////
                    // Handle Inv message
                    (_, SessionStatus::Consolidated, Command::Inv(inv)) => {
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
        debug!("GetPeers message should be sent through the network");
        // Create get peers message
        let get_peers_msg = WitnetMessage::build_get_peers();
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
        // Create AnnounceItems message
        let announce_items_msg = WitnetMessage::build_inv(msg.items);
        // Write message in session
        self.send_message(announce_items_msg);
    }
}

/// Function to try to consolidate session if handshake conditions are met
fn try_consolidate_session(session: &mut Session, ctx: &mut Context<Session>) {
    // Check if HandshakeFlags are all set to true
    if session.handshake_flags.all_true() && session.remote_sender_addr.is_some() {
        // Update session to consolidate status
        update_consolidate(session, ctx);

        // If session type is Outbound, start initial block synchronization
        if let SessionType::Outbound = session.session_type {
            inventory_get_blocks(session, ctx);
        }
    }
}

/// Function to retrieve highest CheckpointBeacon and send GetBlocks message in Session
fn inventory_get_blocks(session: &Session, ctx: &mut Context<Session>) {
    // Get BlocksManager address from registry
    let blocks_manager_addr = System::current().registry().get::<BlocksManager>();
    // Send get highest checkpoint beacon message to BlocksManager
    blocks_manager_addr
        .send(GetHighestCheckpointBeacon)
        .into_actor(session)
        .then(|res, act, ctx| {
            match res {
                Ok(Ok(beacon)) => {
                    // Create get blocks message
                    let get_blocks_msg = WitnetMessage::build_get_blocks(beacon);
                    // Write get blocks message in session
                    act.send_message(get_blocks_msg);

                    actix::fut::ok(())
                }
                _ => {
                    warn!("Get highest checkpoint beacon in Blocks Manager failed");
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
                    debug!("Session successfully consolidated in the Session Manager");
                    // Set status to consolidate
                    act.status = SessionStatus::Consolidated;

                    actix::fut::ok(())
                }
                _ => {
                    warn!("Session consolidate in Session Manager failed");
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
                    info!(
                        "Received ({:?}) peer addresses from PeersManager",
                        addresses.len()
                    );
                    let peers_msg = WitnetMessage::build_peers(&addresses);
                    act.send_message(peers_msg);
                }
                _ => {
                    warn!("Failed to receive peers from PeersManager");
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
    // Get BlocksManager address
    let blocks_manager_addr = System::current().registry().get::<BlocksManager>();

    // Send a message to the BlocksManager to try to add a new block
    blocks_manager_addr.do_send(AddNewBlock { block });
}

/// Function to process an Inv message
fn inventory_process_inv(session: &mut Session, _ctx: &mut Context<Session>, inv: &Inv) {
    // Check how many of the received inventory vectors need to be requested
    let inv_vectors = &inv.inventory;

    // TODO missing check of how many of these items really need to be requested
    let missing_inv_vectors = inv_vectors;

    // Check if there are any vectors to be requested
    if !missing_inv_vectors.is_empty() {
        // Create GetData message with requested inventory vectors
        let get_data_msg = WitnetMessage::build_get_data(missing_inv_vectors.to_vec());

        // Write GetData message in session
        session.send_message(get_data_msg);
    }
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
        let verack = WitnetMessage::build_verack();
        responses.push(verack);
    }
    if !flags.version_tx {
        flags.version_tx = true;
        let version = WitnetMessage::build_version(session.server_addr, session.remote_addr, 0);
        responses.push(version);
    }

    responses
}
/// Function called when GetData message is received
fn send_block_msg(session: &mut Session, ctx: &mut Context<Session>, hash: &Hash) {
    let Hash::SHA256(block_key) = *hash;

    // TODO Use Inventory Manager
    // Add block from storage:
    // Get storage manager actor address
    let storage_manager_addr = System::current().registry().get::<StorageManager>();
    storage_manager_addr
        // Send a message to read the block from the storage
        .send(Get::<Block>::new(block_key.to_vec()))
        .into_actor(session)
        // Process the response
        .then(|res, _act, _ctx| match res {
            Err(e) => {
                // Error when sending message
                error!("Unsuccessful communication with storage manager: {}", e);
                actix::fut::err(())
            }
            Ok(res) => match res {
                Err(e) => {
                    // Storage error
                    error!("Error while getting block from storage: {}", e);
                    actix::fut::err(())
                }
                Ok(res) => actix::fut::ok(res),
            },
        })
        .and_then(|block_from_storage, act, _ctx| {
            // block_from_storage can be None if the storage does not contain that key
            if let Some(block_from_storage) = block_from_storage {
                let header = block_from_storage.header;
                let txns = block_from_storage.txns;

                // Build Block msg
                let block_msg = WitnetMessage::build_block(header, txns);

                // Send Block msg
                act.send_message(block_msg);
            } else {
                warn!("Inventory element not found in Storage");
            }

            actix::fut::ok(())
        })
        .wait(ctx);
}
