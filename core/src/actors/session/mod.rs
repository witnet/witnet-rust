use std::net::SocketAddr;
use std::time::Duration;

use actix::io::FramedWrite;

use log::info;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;

use crate::actors::codec::P2PCodec;
use witnet_data_structures::types::Message as WitnetMessage;
use witnet_p2p::sessions::{SessionStatus, SessionType};

mod actor;

mod handlers;
/// Messages for session
pub mod messages;

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
    /// Server socket address (local peer)
    server_addr: SocketAddr,

    /// Remote socket address (remote server address only if outbound session)
    remote_addr: SocketAddr,

    /// Session type
    session_type: SessionType,

    /// Framed wrapper to send messages through the TCP connection
    framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,

    /// Handshake timeout
    handshake_timeout: Duration,

    /// Session status
    status: SessionStatus,

    /// HandshakeFlags
    handshake_flags: HandshakeFlags,

    /// Remote sender address
    remote_sender_addr: Option<SocketAddr>,
}

/// Session helper methods
impl Session {
    /// Method to create a new session
    pub fn new(
        server_addr: SocketAddr,
        remote_addr: SocketAddr,
        session_type: SessionType,
        framed: FramedWrite<WriteHalf<TcpStream>, P2PCodec>,
        handshake_timeout: Duration,
    ) -> Session {
        Session {
            server_addr,
            remote_addr,
            session_type,
            framed,
            handshake_timeout,
            status: SessionStatus::Unconsolidated,
            handshake_flags: HandshakeFlags::default(),
            remote_sender_addr: None,
        }
    }
    /// Method to send a Witnet message to the remote peer
    fn send_message(&mut self, msg: WitnetMessage) {
        info!(
            "-----> Session ({:?}) sending message: {:?}",
            self.remote_addr, msg
        );
        // Convert WitnetMessage into a vector of bytes
        let bytes: Vec<u8> = msg.into();
        // Convert bytes into BytestMut and send them
        self.framed.write(bytes.into());
    }
}
