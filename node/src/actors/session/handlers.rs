use std::{cmp::Ordering, io::Error, net::SocketAddr};

use actix::{
    io::WriteHandler, ActorContext, ActorFuture, Context, ContextFutureSpawner, Handler,
    StreamHandler, SystemService, WrapFuture,
};
use failure::Fail;
use futures::future;

use witnet_data_structures::{
    builders::from_address,
    chain::{
        Block, CheckpointBeacon, Epoch, Hashable, InventoryEntry, InventoryItem, SuperBlock,
        SuperBlockVote,
    },
    proto::ProtobufConvert,
    transaction::Transaction,
    types::{
        Address, Command, InventoryAnnouncement, InventoryRequest, LastBeacon,
        Message as WitnetMessage, Peers, Version,
    },
};
use witnet_p2p::sessions::{SessionStatus, SessionType};

use super::Session;
use crate::actors::{
    chain_manager::ChainManager,
    codec::BytesMut,
    inventory_manager::InventoryManager,
    messages::{
        AddBlocks, AddCandidates, AddConsolidatedPeer, AddPeers, AddSuperBlockVote, AddTransaction,
        CloseSession, Consolidate, EpochNotification, GetBlocksEpochRange,
        GetHighestCheckpointBeacon, GetItem, GetSuperBlockVotes, PeerBeacon,
        RemoveAddressesFromTried, RequestPeers, SendGetPeers, SendInventoryAnnouncement,
        SendInventoryItem, SendInventoryRequest, SendLastBeacon, SendSuperBlockVote,
        SessionUnitResult,
    },
    peers_manager::PeersManager,
    sessions_manager::SessionsManager,
};
use witnet_util::timestamp::get_timestamp;

#[derive(Debug, Eq, Fail, PartialEq)]
enum HandshakeError {
    #[fail(
        display = "Received beacon is behind our beacon. Current beacon: {:?}, received beacon: {:?}",
        current_beacon, received_beacon
    )]
    PeerBeaconOld {
        current_beacon: LastBeacon,
        received_beacon: LastBeacon,
    },
    #[fail(
        display = "Received beacon is on the same epoch but different block hash. Current beacon: {:?}, received beacon: {:?}",
        current_beacon, received_beacon
    )]
    PeerBeaconDifferentBlockHash {
        current_beacon: LastBeacon,
        received_beacon: LastBeacon,
    },
    #[fail(
        display = "Their epoch is different from ours. Current epoch: {}, received beacon: {:?}",
        current_epoch, received_beacon
    )]
    DifferentEpoch {
        current_epoch: Epoch,
        received_beacon: LastBeacon,
    },
    #[fail(
        display = "Their timestamp is different from ours ({:+} seconds), current timestamp: {}",
        timestamp_diff, current_ts
    )]
    DifferentTimestamp {
        current_ts: i64,
        timestamp_diff: i64,
    },
}

/// Implement WriteHandler for Session
impl WriteHandler<Error> for Session {}

/// Payload for the notification for a specific epoch
#[derive(Debug)]
pub struct EpochPayload;

/// Payload for the notification for all epochs
#[derive(Clone, Debug)]
pub struct EveryEpochPayload;

/// Handler for EpochNotification<EveryEpochPayload>
impl Handler<EpochNotification<EveryEpochPayload>> for Session {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EveryEpochPayload>, ctx: &mut Context<Self>) {
        log::trace!("Periodic epoch notification received {:?}", msg.checkpoint);
        let current_timestamp = get_timestamp();
        log::trace!(
            "Timestamp diff: {}, Epoch timestamp: {}. Current timestamp: {}",
            current_timestamp as i64 - msg.timestamp as i64,
            msg.timestamp,
            current_timestamp
        );

        self.current_epoch = msg.checkpoint;

        if self.blocks_timestamp != 0
            && current_timestamp - self.blocks_timestamp > self.blocks_timeout
        {
            // Get ChainManager address
            let chain_manager_addr = ChainManager::from_registry();

            chain_manager_addr.do_send(AddBlocks { blocks: vec![] });
            log::warn!("Timeout for waiting blocks achieved");
            ctx.stop();
        }
    }
}

/// Implement `StreamHandler` trait in order to use `Framed` with an actor
impl StreamHandler<BytesMut, Error> for Session {
    /// This is main event loop for client requests
    fn handle(&mut self, bytes: BytesMut, ctx: &mut Self::Context) {
        let result = WitnetMessage::from_pb_bytes(&bytes);

        match result {
            Err(err) => {
                log::error!("Error decoding message: {:?}", err);

                ctx.stop();
            }
            Ok(msg) => {
                self.log_received_message(&msg, &bytes);

                // Consensus constants validation between nodes
                if msg.magic != self.magic_number {
                    log::trace!(
                        "Mismatching consensus constants. \
                         Magic number received: {}, Ours: {}",
                        msg.magic,
                        self.magic_number
                    );

                    // Stop this session
                    ctx.stop();
                    return;
                }

                match (self.session_type, self.status, msg.kind) {
                    ////////////////////
                    //   HANDSHAKE    //
                    ////////////////////
                    // Handle Version message
                    (
                        session_type,
                        SessionStatus::Unconsolidated,
                        Command::Version(command_version),
                    ) => {
                        let current_ts = get_timestamp();
                        match handshake_version(
                            self,
                            &command_version,
                            current_ts,
                            self.current_epoch,
                        ) {
                            Ok(msgs) => {
                                for msg in msgs {
                                    self.send_message(msg);
                                }

                                try_consolidate_session(self, ctx);
                            }
                            Err(err) => {
                                if session_type == SessionType::Feeler {
                                    let peers_manager_addr = PeersManager::from_registry();
                                    // Ice the peer that was an error
                                    peers_manager_addr.do_send(RemoveAddressesFromTried {
                                        addresses: vec![self.remote_addr],
                                        ice: true,
                                    });
                                    log::debug!(
                                        "Dropping feeler connection {}: {}",
                                        self.remote_addr,
                                        err
                                    );
                                } else {
                                    log::warn!(
                                        "Dropping {:?} peer {}: {}",
                                        session_type,
                                        self.remote_addr,
                                        err
                                    );
                                }

                                // Stop this session
                                ctx.stop();
                            }
                        }
                    }
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
                        peer_discovery_peers(&peers, self.remote_addr);
                    }
                    ///////////////////////
                    // INVENTORY_REQUEST //
                    ///////////////////////
                    (
                        _,
                        SessionStatus::Consolidated,
                        Command::InventoryRequest(InventoryRequest { inventory }),
                    ) => {
                        let inventory_mngr = InventoryManager::from_registry();
                        let item_requests: Vec<_> = inventory
                            .iter()
                            .map(|item| inventory_mngr.send(GetItem { item: item.clone() }))
                            .collect();

                        future::join_all(item_requests)
                            .into_actor(self)
                            .map_err(|e, _, _| log::error!("Inventory request error: {}", e))
                            .and_then(move |item_responses, session, _| {
                                let mut send_superblock_votes = false;
                                for (i, item_response) in item_responses.into_iter().enumerate() {
                                    match item_response {
                                        Ok(item) => {
                                            if let InventoryItem::Block(block) = &item {
                                                if block.block_header.beacon.checkpoint
                                                    == session
                                                        .last_beacon
                                                        .highest_block_checkpoint
                                                        .checkpoint
                                                {
                                                    send_superblock_votes = true;
                                                }
                                            }

                                            send_inventory_item_msg(session, item)
                                        }
                                        Err(e) => {
                                            // Failed to retrieve item from inventory manager
                                            match inventory[i] {
                                                InventoryEntry::Block(hash) => {
                                                    log::warn!(
                                                        "Inventory request: {}: block {}",
                                                        e,
                                                        hash
                                                    );
                                                }
                                                InventoryEntry::Tx(hash) => {
                                                    log::warn!(
                                                        "Inventory request: {}: transaction {}",
                                                        e,
                                                        hash
                                                    );
                                                }
                                                InventoryEntry::SuperBlock(index) => {
                                                    log::warn!(
                                                        "Inventory request: {}: superblock {}",
                                                        e,
                                                        index
                                                    );
                                                }
                                            }
                                            // Stop block sending if an error occurs
                                            break;
                                        }
                                    }
                                }

                                actix::fut::ok(send_superblock_votes)
                            })
                            .and_then(|send_superblock_votes, session, _ctx| {
                                // If this is the last batch, send to the peer all the superblock votes that are currently stored in
                                // ChainManager. This allows faster synchronization in some cases, because if a node has not
                                // received enough votes, it will revert to the last consolidated superblock and start the
                                // synchronization again.
                                // Note that it is not strictly needed as part of the protocol.
                                let chain_manager_addr = ChainManager::from_registry();
                                let fut = chain_manager_addr
                                    .send(GetSuperBlockVotes)
                                    .into_actor(session)
                                    .map(|res, session, _ctx| match res {
                                        Ok(votes) => {
                                            for vote in votes {
                                                send_superblock_vote(session, vote);
                                            }
                                        }
                                        Err(e) => {
                                            log::error!("Inventory request error (votes): {}", e)
                                        }
                                    })
                                    .map_err(|e, _act, _ctx| {
                                        log::error!("Inventory request error (votes): {}", e)
                                    });

                                if send_superblock_votes {
                                    actix::fut::Either::A(fut)
                                } else {
                                    actix::fut::Either::B(actix::fut::ok(()))
                                }
                            })
                            .wait(ctx);
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

                    /////////////////////////
                    // SUPERBLOCK RECEIVED //
                    /////////////////////////
                    // Handle Block
                    (_, SessionStatus::Consolidated, Command::SuperBlock(superblock)) => {
                        inventory_process_superblock(self, ctx, superblock);
                    }

                    /////////////////
                    // LAST BEACON //
                    /////////////////
                    (
                        SessionType::Inbound,
                        SessionStatus::Consolidated,
                        Command::LastBeacon(last_beacon),
                    ) => {
                        session_last_beacon_inbound(self, ctx, last_beacon);
                    }
                    (
                        SessionType::Outbound,
                        SessionStatus::Consolidated,
                        Command::LastBeacon(last_beacon),
                    ) => {
                        session_last_beacon_outbound(self, ctx, last_beacon);
                    }

                    ////////////////////////////
                    // INVENTORY ANNOUNCEMENT //
                    ////////////////////////////
                    // Handle InventoryAnnouncement message
                    (_, SessionStatus::Consolidated, Command::InventoryAnnouncement(inv)) => {
                        inventory_process_inv(self, &inv);
                    }

                    /////////////////////
                    // SUPERBLOCK VOTE //
                    /////////////////////
                    (_, SessionStatus::Consolidated, Command::SuperBlockVote(sbv)) => {
                        process_superblock_vote(self, sbv)
                    }

                    /////////////////////
                    // NOT SUPPORTED   //
                    /////////////////////
                    (session_type, session_status, msg_type) => {
                        log::warn!(
                            "Message of type {} for session (type: {:?}, status: {:?}) is \
                             not supported",
                            msg_type,
                            session_type,
                            session_status
                        );
                    }
                };
            }
        }
    }
}

/// Handler for GetPeers message (sent by other actors)
impl Handler<SendGetPeers> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, _msg: SendGetPeers, _: &mut Context<Self>) {
        log::trace!("Sending GetPeers message to peer at {:?}", self.remote_addr);
        // Create get peers message
        let get_peers_msg = WitnetMessage::build_get_peers(self.magic_number);
        // Write get peers message in session
        self.send_message(get_peers_msg);
    }
}

/// Handler for SendInventoryAnnouncement message (sent by other actors)
impl Handler<SendInventoryAnnouncement> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: SendInventoryAnnouncement, _: &mut Context<Self>) {
        log::trace!(
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

/// Handler for SendInventoryRequest message (sent by other actors)
impl Handler<SendInventoryRequest> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: SendInventoryRequest, _: &mut Context<Self>) {
        log::trace!(
            "Sending SendInventoryRequest message to peer at {:?}",
            self.remote_addr
        );

        // Try to create InventoryRequest protocol message to request missing inventory vectors
        if let Ok(inv_req_msg) =
            WitnetMessage::build_inventory_request(self.magic_number, msg.items)
        {
            // Send InventoryRequest message through the session network connection
            self.send_message(inv_req_msg);
        }
    }
}

/// Handler for SendInventoryItem message (sent by other actors)
impl Handler<SendInventoryItem> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, msg: SendInventoryItem, _: &mut Context<Self>) {
        log::trace!(
            "Sending SendInventoryItem message to peer at {:?}",
            self.remote_addr
        );
        send_inventory_item_msg(self, msg.item)
    }
}

impl Handler<SendLastBeacon> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, SendLastBeacon { last_beacon }: SendLastBeacon, _ctx: &mut Context<Self>) {
        log::trace!("Sending LastBeacon to peer at {:?}", self.remote_addr);
        send_last_beacon(self, last_beacon);
    }
}

impl Handler<SendSuperBlockVote> for Session {
    type Result = SessionUnitResult;

    fn handle(
        &mut self,
        SendSuperBlockVote { superblock_vote }: SendSuperBlockVote,
        _ctx: &mut Context<Self>,
    ) {
        log::trace!("Sending SuperBlockVote to peer at {:?}", self.remote_addr);
        send_superblock_vote(self, superblock_vote);
    }
}

impl Handler<CloseSession> for Session {
    type Result = SessionUnitResult;

    fn handle(&mut self, _msg: CloseSession, ctx: &mut Context<Self>) {
        ctx.stop();
    }
}

/// Function to try to consolidate session if handshake conditions are met
fn try_consolidate_session(session: &mut Session, ctx: &mut Context<Session>) {
    // Check if HandshakeFlags are all set to true
    if session.handshake_flags.all_true() {
        // Update session to consolidate status
        update_consolidate(session, ctx);
    }
}

// Function to notify the SessionsManager that the session has been consolidated
fn update_consolidate(session: &Session, ctx: &mut Context<Session>) {
    // First evaluate Feeler case
    if session.session_type == SessionType::Feeler {
        // Get peer manager address
        let peers_manager_addr = PeersManager::from_registry();

        // Send AddConsolidatedPeer message to the peers manager
        // Try to add this potential peer in the tried addresses bucket
        peers_manager_addr.do_send(AddConsolidatedPeer {
            // Use the address to which we connected to, not the public address reported by the peer
            address: session.remote_addr,
        });

        // After add peer to tried bucket, this session is not longer useful
        ctx.stop();
    } else {
        // This address is a potential peer to be added to the new bucket
        let potential_new_peer = if session.session_type == SessionType::Inbound {
            session.remote_sender_addr
        } else {
            // In the case of Outbound peers we already know the address of the peer, no need to
            // check their reported public address again
            None
        };

        // Get session manager address
        let session_manager_addr = SessionsManager::from_registry();

        // Register self in session manager. `AsyncContext::wait` register
        // future within context, but context waits until this future resolves
        // before processing any other events.
        session_manager_addr
            .send(Consolidate {
                address: session.remote_addr,
                potential_new_peer,
                session_type: session.session_type,
            })
            .into_actor(session)
            .then(|res, act, ctx| {
                match res {
                    Ok(Ok(_)) => {
                        log::debug!(
                            "Successfully consolidated session {:?} in SessionManager",
                            act.remote_addr
                        );
                        // Set status to consolidate
                        act.status = SessionStatus::Consolidated;

                        actix::fut::ok(())
                    }
                    _ => {
                        log::debug!(
                            "Failed to consolidate session {:?} in SessionManager",
                            act.remote_addr
                        );

                        ctx.stop();

                        actix::fut::err(())
                    }
                }
            })
            .wait(ctx);
    }
}

/// Function called when GetPeers message is received
fn peer_discovery_get_peers(session: &mut Session, ctx: &mut Context<Session>) {
    // Get the address of PeersManager actor
    let peers_manager_addr = PeersManager::from_registry();

    // Start chain of actions
    peers_manager_addr
        // Send RequestPeers message to PeersManager actor
        // This returns a Request Future, representing an asynchronous message sending process
        .send(RequestPeers)
        // Convert a normal future into an ActorFuture
        .into_actor(session)
        // Process the response from PeersManager
        // This returns a FutureResult containing the socket address if present
        .then(|res, act, ctx| {
            match res {
                Ok(Ok(addresses)) => {
                    log::debug!(
                        "Received {} peer addresses from PeersManager",
                        addresses.len()
                    );
                    let peers_msg = WitnetMessage::build_peers(act.magic_number, &addresses);
                    act.send_message(peers_msg);
                }
                _ => {
                    log::warn!("Failed to receive peer addresses from PeersManager");
                    ctx.stop();
                }
            }
            actix::fut::ok(())
        })
        .wait(ctx);
}

/// Function called when Peers message is received
fn peer_discovery_peers(peers: &[Address], src_address: SocketAddr) {
    // Get peers manager address
    let peers_manager_addr = PeersManager::from_registry();

    // Convert array of address to vector of socket addresses
    let addresses = peers.iter().map(from_address).collect();

    // Send AddPeers message to the peers manager
    peers_manager_addr.do_send(AddPeers {
        addresses,
        src_address: Some(src_address),
    });
}

/// Function called when Block message is received
fn inventory_process_block(session: &mut Session, _ctx: &mut Context<Session>, block: Block) {
    // Get ChainManager address
    let chain_manager_addr = ChainManager::from_registry();
    let block_hash = block.hash();

    if session.requested_block_hashes.contains(&block_hash) {
        // Add block to requested_blocks
        session.requested_blocks.insert(block_hash, block);

        if session.requested_blocks.len() == session.requested_block_hashes.len() {
            let mut blocks_vector = Vec::with_capacity(session.requested_blocks.len());
            // Iterate over requested block hashes ordered by epoch
            // TODO: We assume that the received InventoryAnnouncement message returns the list of
            // block hashes ordered by epoch.
            // If that is not the case, we can sort blocks_vector by block.block_header.checkpoint
            for hash in session.requested_block_hashes.drain(..) {
                if let Some(block) = session.requested_blocks.remove(&hash) {
                    blocks_vector.push(block);
                } else {
                    // Assuming that we always clear requested_blocks after mutating
                    // requested_block_hashes, this branch should be unreachable.
                    // But if it happens, immediately exit the for loop and send an empty AddBlocks
                    // message to ChainManager.
                    log::warn!("Unexpected missing block: {}", hash);
                    break;
                }
            }

            // Send a message to the ChainManager to try to add a new block
            chain_manager_addr.do_send(AddBlocks {
                blocks: blocks_vector,
            });

            // Clear requested block structures
            session.blocks_timestamp = 0;
            session.requested_blocks.clear();
            // requested_block_hashes is cleared by using drain(..) above
        }
    } else {
        // If this is not a requested block, assume it is a candidate
        // Send a message to the ChainManager to try to add a new candidate
        chain_manager_addr.do_send(AddCandidates {
            blocks: vec![block],
        });
    }
}

/// Function called when Transaction message is received
fn inventory_process_transaction(
    _session: &mut Session,
    _ctx: &mut Context<Session>,
    transaction: Transaction,
) {
    // Get ChainManager address
    let chain_manager_addr = ChainManager::from_registry();

    // Send a message to the ChainManager to try to add a new transaction
    chain_manager_addr.do_send(AddTransaction { transaction });
}

/// Function called when SuperBlock message is received
fn inventory_process_superblock(
    _session: &mut Session,
    _ctx: &mut Context<Session>,
    superblock: SuperBlock,
) {
    // Get ChainManager address
    let chain_manager_addr = ChainManager::from_registry();

    // Send a message to the ChainManager to try to add a new superblock
    chain_manager_addr.do_send(AddSuperBlock { superblock });
}

/// Function to process an InventoryAnnouncement message
fn inventory_process_inv(session: &mut Session, inv: &InventoryAnnouncement) {
    // Check how many of the received inventory vectors need to be requested
    let inv_entries = &inv.inventory;

    if !session.requested_block_hashes.is_empty() {
        log::warn!("Received InventoryAnnouncement message while processing an older InventoryAnnouncement. Will stop processing the old one.");
    }

    session.requested_block_hashes = inv_entries
        .iter()
        .filter_map(|inv_entry| match inv_entry.clone() {
            InventoryEntry::Block(hash) => Some(hash),
            _ => None,
        })
        .collect();

    // Clear requested block structures. If a different block download was already in process, we
    // may receive some "unrequested" blocks, but that should not break the synchronization.
    session.requested_blocks.clear();
    session.blocks_timestamp = get_timestamp();

    // Try to create InventoryRequest protocol message to request missing inventory vectors
    if let Ok(inv_req_msg) =
        WitnetMessage::build_inventory_request(session.magic_number, inv_entries.to_vec())
    {
        // Send InventoryRequest message through the session network connection
        session.send_message(inv_req_msg);
    }
}

/// Function called when Verack message is received
fn handshake_verack(session: &mut Session) {
    let flags = &mut session.handshake_flags;

    if flags.verack_rx {
        log::debug!("Verack message already received");
    }

    // Set verack_rx flag
    flags.verack_rx = true;
}

fn check_beacon_compatibility(
    current_beacon: &LastBeacon,
    received_beacon: &LastBeacon,
    current_epoch: Epoch,
) -> Result<(), HandshakeError> {
    if received_beacon.highest_block_checkpoint.checkpoint > current_epoch {
        return Err(HandshakeError::DifferentEpoch {
            current_epoch,
            received_beacon: received_beacon.clone(),
        });
    }

    match current_beacon
        .highest_block_checkpoint
        .checkpoint
        .cmp(&received_beacon.highest_block_checkpoint.checkpoint)
    {
        // current_checkpoint < received_checkpoint: received beacon is ahead of us
        Ordering::Less => Ok(()),
        // current_checkpoint > received_checkpoint: received beacon is behind us
        Ordering::Greater => Err(HandshakeError::PeerBeaconOld {
            current_beacon: current_beacon.clone(),
            received_beacon: received_beacon.clone(),
        }),
        // current_checkpoint == received_checkpoint
        Ordering::Equal => {
            if current_beacon.highest_block_checkpoint.hash_prev_block
                == received_beacon.highest_block_checkpoint.hash_prev_block
            {
                // Beacons are equal
                Ok(())
            } else {
                Err(HandshakeError::PeerBeaconDifferentBlockHash {
                    current_beacon: current_beacon.clone(),
                    received_beacon: received_beacon.clone(),
                })
            }
        }
    }
}

/// Function called when Version message is received
fn handshake_version(
    session: &mut Session,
    command_version: &Version,
    current_ts: i64,
    current_epoch: Epoch,
) -> Result<Vec<WitnetMessage>, HandshakeError> {
    // Check timestamp drift
    let received_ts = command_version.timestamp;
    if session.handshake_max_ts_diff != 0
        && (current_ts - received_ts).abs() > session.handshake_max_ts_diff
    {
        return Err(HandshakeError::DifferentTimestamp {
            current_ts,
            timestamp_diff: received_ts - current_ts,
        });
    }

    // Check beacon compatibility
    let current_beacon = &session.last_beacon;
    let received_beacon = &command_version.beacon;

    match session.session_type {
        SessionType::Outbound | SessionType::Feeler => {
            check_beacon_compatibility(current_beacon, received_beacon, current_epoch)?;
        }
        // Do not check beacon for inbound peers
        SessionType::Inbound => {}
    }

    let flags = &mut session.handshake_flags;

    if flags.version_rx {
        log::debug!("Version message already received");
    }

    session.remote_sender_addr = Some(from_address(&command_version.sender_address));

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
            session.public_addr,
            session.remote_addr,
            session.last_beacon.clone(),
        );
        responses.push(version);
    }

    Ok(responses)
}

fn send_inventory_item_msg(session: &mut Session, item: InventoryItem) {
    match item {
        InventoryItem::Block(Block {
            block_header,
            block_sig,
            txns,
        }) => {
            // Build Block msg
            let block_msg =
                WitnetMessage::build_block(session.magic_number, block_header, block_sig, txns);
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
        InventoryItem::SuperBlock(superblock) => {
            // Build Transaction msg
            let superblock_msg = WitnetMessage::build_superblock(session.magic_number, superblock);
            // Send Transaction msg
            session.send_message(superblock_msg);
        }
    }
}

// FIXME(#1366): handle superblock_beacon.
fn session_last_beacon_inbound(
    session: &Session,
    ctx: &mut Context<Session>,
    LastBeacon {
        highest_block_checkpoint:
            CheckpointBeacon {
                checkpoint: received_checkpoint,
                ..
            },
        ..
    }: LastBeacon,
) {
    // TODO: LastBeacon on inbound peers?
    // Get ChainManager address from registry
    let chain_manager_addr = ChainManager::from_registry();
    // Send GetHighestCheckpointBeacon message to ChainManager
    chain_manager_addr
        .send(GetHighestCheckpointBeacon)
        .into_actor(session)
        .then(move |res, act, ctx| {
            match res {
                Ok(Ok(chain_beacon)) => {
                    match received_checkpoint.cmp(&chain_beacon.checkpoint) {
                        Ordering::Greater => {
                            log::warn!(
                                "Received a checkpoint beacon that is ahead of ours ({} > {})",
                                received_checkpoint,
                                chain_beacon.checkpoint
                            );
                        }
                        Ordering::Equal => {
                            log::info!("Our chain is on par with our peer's",);
                        }
                        Ordering::Less => {
                            let range = received_checkpoint..=chain_beacon.checkpoint;

                            chain_manager_addr
                                .send(GetBlocksEpochRange::new_with_const_limit(range))
                                .into_actor(act)
                                .then(|res, act, _ctx| match res {
                                    Ok(Ok(blocks)) => {
                                        // Try to create an Inv protocol message with the items to
                                        // be announced
                                        if let Ok(inv_msg) =
                                            WitnetMessage::build_inventory_announcement(
                                                act.magic_number,
                                                blocks
                                                    .into_iter()
                                                    .map(|(_epoch, hash)| {
                                                        InventoryEntry::Block(hash)
                                                    })
                                                    .collect(),
                                            )
                                        {
                                            // Send Inv message through the session network connection
                                            act.send_message(inv_msg);
                                        };

                                        actix::fut::ok(())
                                    }
                                    _ => {
                                        log::error!("LastBeacon::EpochRange didn't succeeded");

                                        actix::fut::err(())
                                    }
                                })
                                .wait(ctx);
                        }
                    }

                    actix::fut::ok(())
                }
                _ => {
                    log::warn!("Failed to get highest checkpoint beacon from ChainManager");
                    ctx.stop();

                    actix::fut::err(())
                }
            }
        })
        .wait(ctx);
}

fn session_last_beacon_outbound(
    session: &Session,
    _ctx: &mut Context<Session>,
    beacon: LastBeacon,
) {
    SessionsManager::from_registry().do_send(PeerBeacon {
        address: session.remote_addr,
        beacon,
    })
}

fn send_last_beacon(session: &mut Session, last_beacon: LastBeacon) {
    let beacon_msg = WitnetMessage::build_last_beacon(session.magic_number, last_beacon);
    // Send LastBeacon msg
    session.send_message(beacon_msg);
}

/// Function called when the `Session` actor recieves a `SendSuperBlockVote` message
/// Send a `SuperBlockVote` message to this peer
fn send_superblock_vote(session: &mut Session, superblock_vote: SuperBlockVote) {
    let superblock_vote_msg =
        WitnetMessage::build_superblock_vote(session.magic_number, superblock_vote);
    // Send SuperBlockVote msg
    session.send_message(superblock_vote_msg);
}

/// Function called when SuperBlockVote message is received from another peer
fn process_superblock_vote(_session: &mut Session, superblock_vote: SuperBlockVote) {
    // Get ChainManager address
    let chain_manager_addr = ChainManager::from_registry();

    // Send a message to the ChainManager to try to validate this superblock vote
    chain_manager_addr.do_send(AddSuperBlockVote { superblock_vote });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_bootstrap_before_epoch_zero() {
        // Check that when the last beacon has epoch 0 and the current epoch is not 0,
        // the two nodes can peer with each other
        let genesis_hash = "1111111111111111111111111111111111111111111111111111111111111111"
            .parse()
            .unwrap();
        let current_beacon = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: genesis_hash,
                checkpoint: 0,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                hash_prev_block: genesis_hash,
                checkpoint: 0,
            },
        };
        let received_beacon = current_beacon.clone();
        // Before epoch 0, the epoch is set to 0
        let current_epoch = 0;

        assert_eq!(
            check_beacon_compatibility(&current_beacon, &received_beacon, current_epoch),
            Ok(())
        );
    }

    #[test]
    fn handshake_bootstrap_after_epoch_zero() {
        // Check that when the last beacon has epoch 0 but the current epoch is not 0,
        // the two nodes can peer
        let genesis_hash = "1111111111111111111111111111111111111111111111111111111111111111"
            .parse()
            .unwrap();
        let current_beacon = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: genesis_hash,
                checkpoint: 0,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                hash_prev_block: genesis_hash,
                checkpoint: 0,
            },
        };
        let received_beacon = current_beacon.clone();
        let current_epoch = 1;

        assert_eq!(
            check_beacon_compatibility(&current_beacon, &received_beacon, current_epoch),
            Ok(()),
        );
    }

    #[test]
    fn handshake_between_node_at_epoch_0_and_node_at_epoch_1() {
        let genesis_hash = "1111111111111111111111111111111111111111111111111111111111111111"
            .parse()
            .unwrap();
        let hash_block_1 = "aa11111111111111111111111111111111111111111111111111111111111111"
            .parse()
            .unwrap();
        let current_beacon = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: hash_block_1,
                checkpoint: 1,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                hash_prev_block: genesis_hash,
                checkpoint: 0,
            },
        };
        let received_beacon = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: genesis_hash,
                checkpoint: 0,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                hash_prev_block: genesis_hash,
                checkpoint: 0,
            },
        };
        let current_epoch = 1;

        // We cannot peer with the other node
        assert_eq!(
            check_beacon_compatibility(&current_beacon, &received_beacon, current_epoch),
            Err(HandshakeError::PeerBeaconOld {
                current_beacon: current_beacon.clone(),
                received_beacon: received_beacon.clone(),
            })
        );
        // But the other node can peer with us and start syncing
        assert_eq!(
            check_beacon_compatibility(&received_beacon, &current_beacon, current_epoch),
            Ok(())
        );
    }

    #[test]
    fn handshake_between_forked_nodes() {
        let hash_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .parse()
            .unwrap();
        let hash_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .parse()
            .unwrap();
        let genesis_hash = "1111111111111111111111111111111111111111111111111111111111111111"
            .parse()
            .unwrap();
        let current_beacon = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: hash_a,
                checkpoint: 1,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                hash_prev_block: genesis_hash,
                checkpoint: 0,
            },
        };
        let received_beacon = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: hash_b,
                checkpoint: 1,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                hash_prev_block: genesis_hash,
                checkpoint: 0,
            },
        };
        let current_epoch = 1;

        // We cannot peer with the other node
        assert_eq!(
            check_beacon_compatibility(&current_beacon, &received_beacon, current_epoch),
            Err(HandshakeError::PeerBeaconDifferentBlockHash {
                current_beacon: current_beacon.clone(),
                received_beacon: received_beacon.clone(),
            })
        );
        // And the other node cannot peer with us
        assert_eq!(
            check_beacon_compatibility(&received_beacon, &current_beacon, current_epoch),
            Err(HandshakeError::PeerBeaconDifferentBlockHash {
                current_beacon: received_beacon,
                received_beacon: current_beacon,
            })
        );
    }
}
