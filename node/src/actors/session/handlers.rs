use std::{cmp::Ordering, convert::TryFrom, io::Error, net::SocketAddr};

use actix::{
    ActorContext, ActorFutureExt, ActorTryFutureExt, Context, ContextFutureSpawner,
    Handler, io::WriteHandler, StreamHandler, SystemService, WrapFuture,
};
use bytes::BytesMut;
use failure::Fail;
use futures::future::Either;

use witnet_data_structures::{
    builders::from_address,
    chain::{
        Block, CheckpointBeacon, Epoch, InventoryEntry, InventoryItem, SuperBlock, SuperBlockVote,
    },
    get_protocol_version, get_protocol_version_activation_epoch, get_protocol_version_period,
    proto::versioning::{Versioned, VersionedHashable},
    transaction::Transaction,
    types::{
        Address, Command, InventoryAnnouncement, InventoryRequest, LastBeacon,
        Message as WitnetMessage, Peers, ProtocolVersion, ProtocolVersionName, Version,
    },
};
use witnet_p2p::sessions::{SessionStatus, SessionType};
use witnet_util::timestamp::get_timestamp;

use crate::actors::{
    chain_manager::ChainManager,
    inventory_manager::InventoryManager,
    messages::{
        AddBlocks, AddCandidates, AddConsolidatedPeer, AddPeers, AddSuperBlock, AddSuperBlockVote,
        AddTransaction, CloseSession, Consolidate, EpochNotification, GetBlocksEpochRange,
        GetHighestCheckpointBeacon, GetItem, GetSuperBlockVotes, PeerBeacon,
        RemoveAddressesFromTried, RequestPeers, SendGetPeers, SendInventoryAnnouncement,
        SendInventoryItem, SendInventoryRequest, SendLastBeacon, SendProtocolVersions,
        SendSuperBlockVote, SessionUnitResult,
    },
    peers_manager::PeersManager,
    sessions_manager::SessionsManager,
};

use super::Session;

#[derive(Debug, Eq, Fail, PartialEq)]
enum HandshakeError {
    #[fail(
        display = "Received beacon is behind our beacon (or target beacon). Current beacon: {:?}, received beacon: {:?}",
        current_beacon, received_beacon
    )]
    PeerBeaconOld {
        current_beacon: CheckpointBeacon,
        received_beacon: CheckpointBeacon,
    },
    #[fail(
        display = "Received beacon is on the same superepoch but different superblock hash. Current beacon: {:?}, received beacon: {:?}",
        current_beacon, received_beacon
    )]
    PeerBeaconDifferentBlockHash {
        current_beacon: CheckpointBeacon,
        received_beacon: CheckpointBeacon,
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
    #[fail(display = "Received versions message has incompatible protocol version information")]
    IncompatibleProtocolVersion {},
}

/// Implement WriteHandler for Session
impl WriteHandler<Error> for Session {}

/// Payload for the notification for a specific epoch
#[allow(dead_code)]
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
            current_timestamp - msg.timestamp,
            msg.timestamp,
            current_timestamp
        );

        self.current_epoch = msg.checkpoint;

        if self.blocks_timestamp != 0
            && current_timestamp - self.blocks_timestamp > self.config.connections.blocks_timeout
        {
            // Get ChainManager address
            let chain_manager_addr = ChainManager::from_registry();

            // Remove this address from tried bucket and ice it
            self.remove_and_ice_peer();

            chain_manager_addr.do_send(AddBlocks {
                blocks: vec![],
                sender: None,
            });
            log::warn!("Timeout for waiting blocks achieved");
            ctx.stop();
        }
    }
}

/// Implement `StreamHandler` trait in order to use `Framed` with an actor
impl StreamHandler<Result<BytesMut, Error>> for Session {
    /// This is main event loop for client requests
    fn handle(&mut self, res: Result<BytesMut, Error>, ctx: &mut Self::Context) {
        if res.is_err() {
            // TODO: how to handle this error?
            return;
        }

        let bytes = res.unwrap();
        let result = WitnetMessage::from_versioned_pb_bytes(&bytes);

        match result {
            Err(err) => {
                log::error!("Error decoding message: {:?}", err);

                // Remove this address from tried bucket and ice it
                self.remove_and_ice_peer();

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

                    // Remove this address from tried bucket and ice it
                    self.remove_and_ice_peer();

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
                            Ok((msgs, protocol_versions)) => {
                                for msg in msgs {
                                    self.send_message(msg);
                                }

                                try_consolidate_session(self, ctx);
                                session_send_protocol_versions(self, protocol_versions);
                            }
                            Err(err) => {
                                if let HandshakeError::DifferentTimestamp { .. }
                                | HandshakeError::DifferentEpoch { .. }
                                | HandshakeError::IncompatibleProtocolVersion {} = err
                                {
                                    // Remove this address from tried bucket and ice it
                                    self.remove_and_ice_peer();
                                } else if session_type == SessionType::Feeler
                                    || session_type == SessionType::Outbound
                                {
                                    // Ice the peer who failed to complete a successful handshake, except in epochs
                                    // where superblock is updated.
                                    // During superblock updates, there will be nodes that are perfectly synced
                                    // yet they are in the process of updating their superblock field for handshaking,
                                    // so mistakenly icing them would be wrong.
                                    if self.current_epoch % 10 != 0 {
                                        // Remove this address from tried bucket and ice it
                                        self.remove_and_ice_peer();
                                    }
                                }

                                if session_type == SessionType::Feeler {
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
                        peer_discovery_peers(self, ctx, &peers, self.remote_addr);
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

                        futures::future::try_join_all(item_requests)
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
                                    .map_ok(|res, session, _ctx| match res {
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
                                    Either::Left(fut)
                                } else {
                                    Either::Right(actix::fut::ok(()))
                                }
                            })
                            .map(|_res: Result<(), ()>, _act, _ctx| ())
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

        self.expected_peers_msg = self.expected_peers_msg.saturating_add(1);
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
                    }
                    _ => {
                        log::debug!(
                            "Failed to consolidate session {:?} in SessionManager",
                            act.remote_addr
                        );

                        // Remove this address from tried bucket and ice it
                        act.remove_and_ice_peer();

                        ctx.stop();
                    }
                }

                actix::fut::ready(())
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
            actix::fut::ready(())
        })
        .wait(ctx);
}

/// Function called when Peers message is received
fn peer_discovery_peers(
    session: &mut Session,
    ctx: &mut Context<Session>,
    peers: &[Address],
    src_address: SocketAddr,
) {
    let peers_requested = session.expected_peers_msg > 0;

    // Get peers manager address
    let peers_manager_addr = PeersManager::from_registry();

    if peers_requested {
        session.expected_peers_msg -= 1;

        // Convert array of address to vector of socket addresses
        let addresses: Vec<SocketAddr> = peers.iter().map(from_address).collect();

        log::debug!(
            "Received {} peer addresses from {}",
            addresses.len(),
            src_address
        );

        // Send AddPeers message to the peers manager
        peers_manager_addr.do_send(AddPeers {
            addresses,
            src_address: Some(src_address),
        });
    } else {
        log::debug!("{} sent unwanted \"peers\"(possibly attack?)", src_address,);

        // If the "peers" messages was unwanted one, put the peer into iced
        peers_manager_addr.do_send(RemoveAddressesFromTried {
            addresses: vec![src_address],
            ice: true,
        });

        // And stop the actor
        ctx.stop()
    }
}

/// Function called when Block message is received
fn inventory_process_block(session: &mut Session, _ctx: &mut Context<Session>, block: Block) {
    // Get ChainManager address
    let chain_manager_addr = ChainManager::from_registry();
    let current_protocol = get_protocol_version(Some(block.block_header.beacon.checkpoint));
    let current_block_hash = block.versioned_hash(current_protocol);

    // This quickly checks if the hash of the received block matches a former inventory request.
    // If it doesn't, it will retry with the next protocol version, just in case a protocol version
    // change happened in the middle of a synchronization chunk.
    // Otherwise, treat the block as a block candidate.
    let block_hash = if session.requested_block_hashes.contains(&current_block_hash) {
        Some(current_block_hash)
    } else {
        let upcoming_protocol = current_protocol.next();

        // Optimize for the case where there's no upcoming protocol to avoid a redundant and
        // potentially costly hash operation.
        if upcoming_protocol > current_protocol {
            let upcoming_block_hash = block.versioned_hash(upcoming_protocol);

            if session
                .requested_block_hashes
                .contains(&upcoming_block_hash)
            {
                Some(upcoming_block_hash)
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some(block_hash) = block_hash {
        // Add block to requested_blocks
        session.requested_blocks.insert(block_hash, block);

        if session.requested_blocks.len() == session.requested_block_hashes.len() {
            // Iterate over requested block hashes ordered by epoch
            // TODO: We assume that the received InventoryAnnouncement message returns the list of
            //  block hashes ordered by epoch.
            //  If that is not the case, we can sort blocks_vector by block.block_header.checkpoint
            let blocks_vector = session
                .requested_block_hashes
                .drain(..)
                .map_while(|hash| {
                    if let Some(block) = session.requested_blocks.remove(&hash) {
                        Some(block)
                    } else {
                        // Assuming that we always clear requested_blocks after mutating
                        // requested_block_hashes, this branch should be unreachable.
                        // But if it happens, immediately exit the iterator and send an empty AddBlocks
                        // message to ChainManager.
                        log::warn!("Unexpected missing block: {}", hash);

                        None
                    }
                })
                .collect();

            // Send a message to the ChainManager to try to add a new block
            chain_manager_addr.do_send(AddBlocks {
                blocks: blocks_vector,
                sender: Some(session.remote_addr),
            });

            // Clear requested block structures
            // Although `requested_block_hashes` is cleared by using drain(..) above, the `.clear()`
            // is still needed because of corner cases, and also for the event of a protocol upgrade
            // happening in the middle of a synchronization chunk, where we may be pushing a hash
            // that is using an old version of the protocol, but draining using a different hash
            // that uses a newer version.
            session.blocks_timestamp = 0;
            session.requested_blocks.clear();

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
    chain_manager_addr.do_send(AddTransaction {
        transaction,
        broadcast_flag: true,
    });
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
    if !session.requested_block_hashes.is_empty() {
        log::warn!("Received InventoryAnnouncement message while processing an older InventoryAnnouncement. Will stop processing the old one.");
    }

    let limit = match usize::try_from(session.config.connections.requested_blocks_batch_limit) {
        // Greater than usize::MAX: set to usize::MAX
        Err(_) => usize::MAX,
        // 0 means unlimited
        Ok(0) => usize::MAX,
        Ok(limit) => {
            // Small limits break the synchronization, so force a minimum value of 2 times the
            // superblock period
            let min_limit = 2 * usize::from(session.config.consensus_constants.superblock_period);
            std::cmp::max(limit, min_limit)
        }
    };

    session.requested_block_hashes = inv
        .inventory
        .iter()
        .filter_map(|inv_entry| match inv_entry {
            InventoryEntry::Block(hash) => Some(*hash),
            _ => None,
        })
        .take(limit)
        .collect();

    // Clear requested block structures. If a different block download was already in process, we
    // may receive some "unrequested" blocks, but that should not break the synchronization.
    session.requested_blocks.clear();
    session.blocks_timestamp = get_timestamp();

    // Try to create InventoryRequest protocol message to request missing inventory vectors
    if let Ok(inv_req_msg) = WitnetMessage::build_inventory_request(
        session.magic_number,
        session
            .requested_block_hashes
            .iter()
            .map(|hash| InventoryEntry::Block(*hash))
            .collect(),
    ) {
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

// If a peer is on a protocol version with faster block times, epochs won't align
// Attempt to recaculate the epoch using the protocol version from the other peer
fn recalculate_epoch(
    time_since_genesis: i64,
    checkpoints_period: u16,
    wit2_protocol_version: Option<ProtocolVersion>,
) -> Epoch {
    match wit2_protocol_version {
        Some(protocol) => {
            let seconds_to_wit2 = protocol.activation_epoch * u32::from(checkpoints_period);
            let seconds_since_wit2 = time_since_genesis as u32 - seconds_to_wit2;
            let epochs_since_wit2 = seconds_since_wit2 / u32::from(protocol.checkpoint_period);

            protocol.activation_epoch + epochs_since_wit2
        }
        None => (time_since_genesis / i64::from(checkpoints_period))
            .try_into()
            .unwrap(),
    }
}

fn check_beacon_compatibility(
    current_beacon: &LastBeacon,
    received_beacon: &LastBeacon,
    current_epoch: Epoch,
    time_since_genesis: i64,
    checkpoints_period: u16,
    target_superblock_beacon: Option<CheckpointBeacon>,
    wit2_protocol_version: Option<ProtocolVersion>,
) -> Result<(), HandshakeError> {
    if received_beacon.highest_block_checkpoint.checkpoint > current_epoch {
        let recalculated_epoch = recalculate_epoch(
            time_since_genesis,
            checkpoints_period,
            wit2_protocol_version,
        );
        log::debug!(
            "Recalculated epoch {} -> {}",
            current_epoch,
            recalculated_epoch
        );
        if received_beacon.highest_block_checkpoint.checkpoint > recalculated_epoch {
            return Err(HandshakeError::DifferentEpoch {
                current_epoch,
                received_beacon: received_beacon.clone(),
            });
        }
    }

    // In order to improve the synchronization process, if we have information about the last
    // superblock consensus achieved by more than 2/3 of the signing committee, we will use it
    // to check the beacon compatibility instead of our own beacon
    let my_beacon = if let Some(target_beacon) = target_superblock_beacon {
        target_beacon
    } else {
        current_beacon.highest_superblock_checkpoint
    };

    match my_beacon
        .checkpoint
        .cmp(&received_beacon.highest_superblock_checkpoint.checkpoint)
    {
        // current_checkpoint < received_checkpoint: received beacon is ahead of us
        Ordering::Less => Ok(()),
        // current_checkpoint > received_checkpoint: received beacon is behind us
        Ordering::Greater => {
            log::trace!(
                "Current SuperBlock beacon: {:?}",
                current_beacon.highest_superblock_checkpoint
            );
            log::trace!("Target SuperBlock beacon: {:?}", my_beacon);

            Err(HandshakeError::PeerBeaconOld {
                current_beacon: my_beacon,
                received_beacon: received_beacon.highest_superblock_checkpoint,
            })
        }
        // current_checkpoint == received_checkpoint
        Ordering::Equal => {
            if my_beacon.hash_prev_block
                == received_beacon
                    .highest_superblock_checkpoint
                    .hash_prev_block
            {
                // Beacons are equal
                Ok(())
            } else {
                log::trace!(
                    "Current SuperBlock beacon: {:?}",
                    current_beacon.highest_superblock_checkpoint
                );
                log::trace!("Target SuperBlock beacon: {:?}", my_beacon);

                Err(HandshakeError::PeerBeaconDifferentBlockHash {
                    current_beacon: my_beacon,
                    received_beacon: received_beacon.highest_superblock_checkpoint,
                })
            }
        }
    }
}

fn check_protocol_version_compatibility(
    received_protocol_versions: &Vec<ProtocolVersion>,
) -> Result<Vec<ProtocolVersion>, HandshakeError> {
    let mut protocol_versions = vec![];
    for protocol in received_protocol_versions {
        // Protocol version not activated yet
        if protocol.activation_epoch == u32::MAX || protocol.checkpoint_period == u16::MAX {
            log::debug!("Received inactive protocol {:?}", protocol.version);
            continue;
        }

        // Check already registered protocols for incompatibilities
        let protocol_activation_epoch =
            get_protocol_version_activation_epoch(protocol.version.into());
        let protocol_period = get_protocol_version_period(protocol.version.into());
        if protocol_activation_epoch != u32::MAX
            && protocol_activation_epoch != protocol.activation_epoch
        {
            log::debug!(
                "Received protocol {:?} with an incompatible activation epoch",
                protocol.version
            );
            return Err(HandshakeError::IncompatibleProtocolVersion {});
        } else if protocol_period != u16::MAX && protocol_period != protocol.checkpoint_period {
            log::debug!(
                "Received protocol {:?} with an incompatible activation epoch",
                protocol.checkpoint_period
            );
            return Err(HandshakeError::IncompatibleProtocolVersion {});
        }

        protocol_versions.push(protocol.clone());
    }

    Ok(protocol_versions)
}

/// Check that the received timestamp is close enough to the current timestamp
fn check_timestamp_drift(
    current_ts: i64,
    received_ts: i64,
    max_ts_diff: i64,
) -> Result<(), HandshakeError> {
    if max_ts_diff == 0 {
        return Ok(());
    }

    let valid_ts_range =
        current_ts.saturating_sub(max_ts_diff)..=current_ts.saturating_add(max_ts_diff);
    if !valid_ts_range.contains(&received_ts) {
        return Err(HandshakeError::DifferentTimestamp {
            current_ts,
            timestamp_diff: received_ts.saturating_sub(current_ts),
        });
    }

    Ok(())
}

/// Function called when Version message is received
fn handshake_version(
    session: &mut Session,
    command_version: &Version,
    current_ts: i64,
    current_epoch: Epoch,
) -> Result<(Vec<WitnetMessage>, Vec<ProtocolVersion>), HandshakeError> {
    // Check that the received timestamp is close enough to the current timestamp
    let received_ts = command_version.timestamp;
    let max_ts_diff = session.config.connections.handshake_max_ts_diff;
    check_timestamp_drift(current_ts, received_ts, max_ts_diff)?;

    // Check beacon compatibility
    let current_beacon = &session.last_beacon;
    let received_beacon = &command_version.beacon;

    let received_protocol_versions = &command_version.protocol_versions;

    let protocol_versions = match session.session_type {
        SessionType::Outbound | SessionType::Feeler => {
            let versions = check_protocol_version_compatibility(received_protocol_versions)?;

            let mut wit2_protocol_version = None;
            for protocol in versions.iter() {
                if protocol.version == ProtocolVersionName::V2_0(true) {
                    wit2_protocol_version = Some(protocol.clone());
                    break;
                }
            }
            let time_since_genesis =
                current_ts - session.config.consensus_constants.checkpoint_zero_timestamp;
            check_beacon_compatibility(
                current_beacon,
                received_beacon,
                current_epoch,
                time_since_genesis,
                session.config.consensus_constants.checkpoints_period,
                session.superblock_beacon_target,
                wit2_protocol_version,
            )?;

            versions
        }
        // Do not check beacon for inbound peers, but do check protocol versions
        SessionType::Inbound => check_protocol_version_compatibility(received_protocol_versions)?,
    };

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

    Ok((responses, protocol_versions))
}

fn send_inventory_item_msg(session: &mut Session, item: InventoryItem) {
    match item {
        InventoryItem::Block(Block {
            block_header,
            block_sig,
            txns,
            ..
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
            // Build SuperBlock msg
            let superblock_msg = WitnetMessage::build_superblock(session.magic_number, superblock);
            // Send SuperBlock msg
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
                            let init = received_checkpoint + 1;
                            let range = init..=chain_beacon.checkpoint;

                            chain_manager_addr
                                .send(GetBlocksEpochRange::new_with_const_limit(range))
                                .into_actor(act)
                                .then(|res, act, _ctx| {
                                    match res {
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
                                            }
                                        }
                                        _ => {
                                            log::error!("LastBeacon::EpochRange didn't succeeded");
                                        }
                                    }

                                    actix::fut::ready(())
                                })
                                .wait(ctx);
                        }
                    }
                }
                _ => {
                    log::warn!("Failed to get highest checkpoint beacon from ChainManager");
                    ctx.stop();
                }
            }

            actix::fut::ready(())
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

fn session_send_protocol_versions(session: &Session, protocol_versions: Vec<ProtocolVersion>) {
    // Only send unregistered protocol versions
    let unregistered_protocol_versions: Vec<_> = protocol_versions
        .into_iter()
        .filter(|protocol| {
            get_protocol_version_activation_epoch(protocol.version.into()) == u32::MAX
        })
        .collect();
    if !unregistered_protocol_versions.is_empty() {
        SessionsManager::from_registry().do_send(SendProtocolVersions {
            address: session.remote_addr,
            protocol_versions: unregistered_protocol_versions,
        })
    }
}

#[cfg(test)]
mod tests {
    use witnet_data_structures::chain::Hash;

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
        // Before epoch 0, the epoch is set to 0
        let current_epoch = 0;

        assert_eq!(
            check_beacon_compatibility(
                &current_beacon,
                &current_beacon,
                current_epoch,
                1_000_000_000,
                45,
                None,
                None
            ),
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
        let current_epoch = 1;

        assert_eq!(
            check_beacon_compatibility(
                &current_beacon,
                &current_beacon,
                current_epoch,
                1_000_000_000,
                45,
                None,
                None
            ),
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

        // Both nodes can peer because they share the same superblock
        assert_eq!(
            check_beacon_compatibility(
                &current_beacon,
                &received_beacon,
                current_epoch,
                1_000_000_000,
                45,
                None,
                None
            ),
            Ok(())
        );
        assert_eq!(
            check_beacon_compatibility(
                &received_beacon,
                &current_beacon,
                current_epoch,
                1_000_000_000,
                45,
                None,
                None
            ),
            Ok(())
        );
    }

    #[test]
    fn handshake_between_node_at_superepoch_0_and_node_at_superepoch_1() {
        let genesis_hash = "1111111111111111111111111111111111111111111111111111111111111111"
            .parse()
            .unwrap();
        let hash_block_10 = "aa11111111111111111111111111111111111111111111111111111111111111"
            .parse()
            .unwrap();
        let hash_superblock_1 = "aaaa111111111111111111111111111111111111111111111111111111111111"
            .parse()
            .unwrap();
        let current_beacon = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: hash_block_10,
                checkpoint: 10,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                hash_prev_block: hash_superblock_1,
                checkpoint: 1,
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
        let current_epoch = 10;

        // We cannot peer with the other node
        assert_eq!(
            check_beacon_compatibility(
                &current_beacon,
                &received_beacon,
                current_epoch,
                1_000_000_000,
                45,
                None,
                None
            ),
            Err(HandshakeError::PeerBeaconOld {
                current_beacon: current_beacon.highest_superblock_checkpoint,
                received_beacon: received_beacon.highest_superblock_checkpoint,
            })
        );
        // But the other node can peer with us and start syncing
        assert_eq!(
            check_beacon_compatibility(
                &received_beacon,
                &current_beacon,
                current_epoch,
                1_000_000_000,
                45,
                None,
                None
            ),
            Ok(())
        );
    }

    #[test]
    fn handshake_between_superforked_nodes() {
        let hash_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .parse()
            .unwrap();
        let hash_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .parse()
            .unwrap();
        let current_beacon = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: Hash::default(),
                checkpoint: 10,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                hash_prev_block: hash_a,
                checkpoint: 1,
            },
        };
        let received_beacon = LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: Hash::default(),
                checkpoint: 10,
            },
            highest_superblock_checkpoint: CheckpointBeacon {
                hash_prev_block: hash_b,
                checkpoint: 1,
            },
        };
        let current_epoch = 10;

        // We cannot peer with the other node
        assert_eq!(
            check_beacon_compatibility(
                &current_beacon,
                &received_beacon,
                current_epoch,
                1_000_000_000,
                45,
                None,
                None
            ),
            Err(HandshakeError::PeerBeaconDifferentBlockHash {
                current_beacon: current_beacon.highest_superblock_checkpoint,
                received_beacon: received_beacon.highest_superblock_checkpoint,
            })
        );
        // And the other node cannot peer with us
        assert_eq!(
            check_beacon_compatibility(
                &received_beacon,
                &current_beacon,
                current_epoch,
                1_000_000_000,
                45,
                None,
                None
            ),
            Err(HandshakeError::PeerBeaconDifferentBlockHash {
                current_beacon: received_beacon.highest_superblock_checkpoint,
                received_beacon: current_beacon.highest_superblock_checkpoint,
            })
        );
    }

    #[test]
    fn timestamp_drift_max_one_second() {
        // When max_ts_diff = 1, the valid timestamps are the ones in [1000 - 1, 1000 + 1] inclusive
        let max_ts_diff = 1;
        let current_ts = 1000;
        let valid_ts = [999, 1000, 1001];
        for received_ts in &valid_ts {
            assert_eq!(
                check_timestamp_drift(current_ts, *received_ts, max_ts_diff),
                Ok(()),
                "{}",
                received_ts
            );
        }
        let invalid_ts = [
            997,
            998,
            1002,
            1003,
            0,
            -1,
            i64::MIN,
            i64::MIN + 1,
            i64::MAX,
            i64::MAX - 1,
        ];
        for received_ts in &invalid_ts {
            assert!(
                check_timestamp_drift(current_ts, *received_ts, max_ts_diff).is_err(),
                "{}",
                received_ts
            );
        }
    }

    #[test]
    fn timestamp_drift_max_zero_seconds() {
        // When max_ts_diff = 0, all received_ts are valid
        let max_ts_diff = 0;
        let current_ts = 1000;
        let valid_ts = [
            997,
            998,
            999,
            1000,
            1001,
            1002,
            1003,
            0,
            -1,
            i64::MIN,
            i64::MIN + 1,
            i64::MAX,
            i64::MAX - 1,
        ];
        for received_ts in &valid_ts {
            assert_eq!(
                check_timestamp_drift(current_ts, *received_ts, max_ts_diff),
                Ok(()),
                "{}",
                received_ts
            );
        }
    }

    #[test]
    fn timestamp_drift_error_overflow() {
        // When max_ts_diff = 1000, the valid timestamps are the ones in [0, 2000] inclusive
        let max_ts_diff = 1000;
        let current_ts = 1000;

        // Timestamps from the past have negative diff
        let received_ts = -1;
        assert_eq!(
            check_timestamp_drift(current_ts, received_ts, max_ts_diff),
            Err(HandshakeError::DifferentTimestamp {
                current_ts,
                timestamp_diff: -1001,
            }),
            "{}",
            received_ts
        );

        // Timestamps from the future have positive diff
        let received_ts = 2001;
        assert_eq!(
            check_timestamp_drift(current_ts, received_ts, max_ts_diff),
            Err(HandshakeError::DifferentTimestamp {
                current_ts,
                timestamp_diff: 1001,
            }),
            "{}",
            received_ts
        );

        // If the difference in timestamps is greater in magnitude than i64::MIN,
        // it is saturated to i64::MIN
        let received_ts = i64::MIN;
        assert_eq!(
            check_timestamp_drift(current_ts, received_ts, max_ts_diff),
            Err(HandshakeError::DifferentTimestamp {
                current_ts,
                timestamp_diff: i64::MIN,
            }),
            "{}",
            received_ts
        );
    }

    #[test]
    fn test_epoch_recalculation() {
        let wit2_protocol = ProtocolVersion {
            version: ProtocolVersionName::V2_0(true),
            activation_epoch: 40,
            checkpoint_period: 20,
        };

        let checkpoints_period: u16 = 45;

        let time_since_genesis = i64::from(42 * checkpoints_period) + 16;
        let recalculated_epoch = recalculate_epoch(
            time_since_genesis,
            checkpoints_period,
            Some(wit2_protocol.clone()),
        );
        assert_eq!(45, recalculated_epoch);

        let time_since_genesis = i64::from(42 * checkpoints_period) + 44;
        let recalculated_epoch = recalculate_epoch(
            time_since_genesis,
            checkpoints_period,
            Some(wit2_protocol.clone()),
        );
        assert_eq!(46, recalculated_epoch);

        let time_since_genesis = i64::from(43 * checkpoints_period) + 14;
        let recalculated_epoch = recalculate_epoch(
            time_since_genesis,
            checkpoints_period,
            Some(wit2_protocol.clone()),
        );
        assert_eq!(47, recalculated_epoch);

        let time_since_genesis = i64::from(43 * checkpoints_period) + 33;
        let recalculated_epoch = recalculate_epoch(
            time_since_genesis,
            checkpoints_period,
            Some(wit2_protocol.clone()),
        );
        assert_eq!(48, recalculated_epoch);
    }
}
