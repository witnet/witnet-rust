use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use actix::{io::FramedWrite, SystemService};

use ansi_term::Color::Green;

use witnet_config::config::Config;
use witnet_data_structures::{
    chain::{Block, CheckpointBeacon, Epoch, Hash},
    proto::ProtobufConvert,
    types::{Command, LastBeacon, Message as WitnetMessage},
};
use witnet_p2p::sessions::{SessionStatus, SessionType};

use crate::actors::{
    codec::P2PCodec,
    messages::{LogMessage, RemoveAddressesFromTried},
    peers_manager::PeersManager,
    sessions_manager::SessionsManager,
};
use bytes::BytesMut;
use tokio::net::tcp::OwnedWriteHalf;

mod actor;

mod handlers;

/// HandshakeFlags
#[derive(Default)]
struct HandshakeFlags {
    /// Flag to indicate that a version message was sent
    version_tx: bool,
    /// Flag to indicate that a version message was received
    version_rx: bool,
    /// Flag to indicate that a verack message was sent
    verack_tx: bool,
    /// Flag to indicate that a verack message was received
    verack_rx: bool,
}

/// HandshakeFlags helper methods
impl HandshakeFlags {
    // Auxiliary function to check if all flags are set to true
    fn all_true(&self) -> bool {
        self.verack_tx && self.verack_rx && self.version_tx && self.version_rx
    }
}

/// Session representing a TCP connection
pub struct Session {
    /// Public address of the node (the one used by other peers to connect to ours)
    public_addr: Option<SocketAddr>,

    /// Remote socket address (remote server address only if outbound session)
    remote_addr: SocketAddr,

    /// Session type
    session_type: SessionType,

    /// Framed wrapper to send messages through the TCP connection
    framed: FramedWrite<BytesMut, OwnedWriteHalf, P2PCodec>,

    /// Session status
    status: SessionStatus,

    /// HandshakeFlags
    handshake_flags: HandshakeFlags,

    /// Remote sender address
    remote_sender_addr: Option<SocketAddr>,

    /// Magic number
    magic_number: u16,

    /// Current epoch
    current_epoch: Epoch,

    /// Current top of the chain
    last_beacon: LastBeacon,

    /// Requested block hashes vector
    requested_block_hashes: Vec<Hash>,

    /// HashMap with requested blocks
    requested_blocks: HashMap<Hash, Block>,

    /// Timestamp for requested blocks
    blocks_timestamp: i64,

    /// Reference to config
    config: Arc<Config>,

    /// Expected number of "peers" message from this peer
    expected_peers_msg: u8,

    /// Superblock beacon target
    superblock_beacon_target: Option<CheckpointBeacon>,
}

impl Drop for Session {
    fn drop(&mut self) {
        log::trace!("Dropping Session");
        // Do not stop the system if one session panics, panics in session are handled in the same
        // way as a session disconnect.
        //stop_system_if_panicking("Session");
    }
}

/// Session helper methods
impl Session {
    /// Method to create a new session
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        public_addr: Option<SocketAddr>,
        remote_addr: SocketAddr,
        session_type: SessionType,
        framed: FramedWrite<BytesMut, OwnedWriteHalf, P2PCodec>,
        magic_number: u16,
        current_epoch: Epoch,
        last_beacon: LastBeacon,
        config: Arc<Config>,
        superblock_beacon_target: Option<CheckpointBeacon>,
    ) -> Session {
        Session {
            public_addr,
            remote_addr,
            session_type,
            framed,
            status: SessionStatus::Unconsolidated,
            handshake_flags: HandshakeFlags::default(),
            remote_sender_addr: None,
            magic_number,
            current_epoch,
            last_beacon,
            requested_block_hashes: vec![],
            requested_blocks: HashMap::new(),
            blocks_timestamp: 0,
            config,
            expected_peers_msg: 0,
            superblock_beacon_target,
        }
    }

    /// Method to send a Witnet message to the remote peer
    fn send_message(&mut self, msg: WitnetMessage) {
        // Convert WitnetMessage into a vector of bytes
        match ProtobufConvert::to_pb_bytes(&msg) {
            Ok(bytes) => {
                match msg.kind {
                    Command::Transaction(_) | Command::Block(_) | Command::SuperBlockVote(_) => {
                        let log_data = format!(
                            "{} Sending {} message ({} bytes)",
                            Green.bold().paint("[>]"),
                            Green.bold().paint(msg.kind.to_string()),
                            bytes.len(),
                        );
                        SessionsManager::from_registry().do_send(LogMessage {
                            log_data,
                            addr: self.remote_addr,
                        })
                    }
                    _ => {
                        log::debug!(
                            "{} Sending  {} message to session {:?} ({} bytes)",
                            Green.bold().paint("[>]"),
                            Green.bold().paint(msg.kind.to_string()),
                            self.remote_addr,
                            bytes.len(),
                        );
                    }
                }
                log::trace!("\t{:?}", msg);
                self.framed.write(bytes.as_slice().into());
            }
            Err(e) => {
                log::error!(
                    "Error sending {} message to session {:?}: {}",
                    msg.kind,
                    self.remote_addr,
                    e,
                );
                log::trace!("\t{:?}", msg);
            }
        }
    }
    // This method is useful to align the logs from receive_message with logs from send_message
    fn log_received_message(&self, msg: &WitnetMessage, bytes: &[u8]) {
        match msg.kind {
            Command::Transaction(_) | Command::Block(_) | Command::SuperBlockVote(_) => {
                let log_data = format!(
                    "{} Received {} message ({} bytes)",
                    Green.bold().paint("[<]"),
                    Green.bold().paint(msg.kind.to_string()),
                    bytes.len(),
                );
                SessionsManager::from_registry().do_send(LogMessage {
                    log_data,
                    addr: self.remote_addr,
                })
            }
            _ => {
                log::debug!(
                    "{} Received {} message from session {:?} ({} bytes)",
                    Green.bold().paint("[<]"),
                    Green.bold().paint(msg.kind.to_string()),
                    self.remote_addr,
                    bytes.len(),
                );
            }
        }

        log::trace!("\t{:?}", msg);
    }

    // Remove this address from tried bucket and move to the ice bucket
    fn remove_and_ice_peer(&self) {
        let peers_manager_addr = PeersManager::from_registry();
        peers_manager_addr.do_send(RemoveAddressesFromTried {
            addresses: vec![self.remote_addr],
            ice: true,
        });
    }
}
