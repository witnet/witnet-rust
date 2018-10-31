//! Library for managing the node sessions, including outbound and inbound sessions

/// Errors module
pub mod error;

/// Bounded sessions module
pub mod bounded_sessions;

use std::net::SocketAddr;

use crate::sessions::bounded_sessions::BoundedSessions;
use crate::sessions::error::SessionsResult;

/// Session type
#[derive(Copy, Clone, Debug)]
pub enum SessionType {
    /// Inbound session
    Inbound,
    /// Outbound session
    Outbound,
}

/// Session Status (used for bootstrapping)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SessionStatus {
    /// Recently created session (no handshake yet)
    Unconsolidated,
    /// Session with successful handshake
    Consolidated,
}

/// Sessions struct contains:
/// - server address used to listen to incoming connections
/// - lists of of inbound and outbound sessions parametrized with their reference (T)
pub struct Sessions<T> {
    /// Server address listening to incoming connections
    pub server_address: Option<SocketAddr>,
    /// Inbound sessions: __untrusted__ peers that connect to the server
    pub inbound_sessions: BoundedSessions<T>,
    /// Outbound sessions: __known__ peers that the node is connected
    pub outbound_sessions: BoundedSessions<T>,
}

/// Default trait implementation
impl<T> Default for Sessions<T> {
    fn default() -> Self {
        Self {
            server_address: None,
            inbound_sessions: BoundedSessions::default(),
            outbound_sessions: BoundedSessions::default(),
        }
    }
}

impl<T> Sessions<T> {
    /// Method to get the sessions map by a session type
    fn get_sessions_by_type(&mut self, session_type: SessionType) -> &mut BoundedSessions<T> {
        match session_type {
            SessionType::Inbound => &mut self.inbound_sessions,
            SessionType::Outbound => &mut self.outbound_sessions,
        }
    }
    /// Method to set the server address
    pub fn set_server_address(&mut self, server_adress: SocketAddr) {
        self.server_address = Some(server_adress);
    }
    /// Method to set the sessions limits
    pub fn set_limits(&mut self, inbound_limit: u16, outbound_limit: u16) {
        self.inbound_sessions.set_limit(inbound_limit);
        self.outbound_sessions.set_limit(outbound_limit);
    }
    /// Method to check if a socket address is eligible as outbound peer
    pub fn is_outbound_address_eligible(&self, candidate_addr: SocketAddr) -> bool {
        // Check if address is already used as outbound session
        let is_outbound = self
            .outbound_sessions
            .collection
            .contains_key(&candidate_addr);
        // Check if address is the server address
        let is_server =
            self.server_address.is_some() && self.server_address.unwrap() == candidate_addr;

        // Return true if the address has not been used as outbound session or server address
        !is_outbound && !is_server
    }
    /// Method to insert a new session
    pub fn register_session(
        &mut self,
        session_type: SessionType,
        address: SocketAddr,
        reference: T,
    ) -> SessionsResult<()> {
        // Get map to insert session to
        let sessions = self.get_sessions_by_type(session_type);

        // Register session and return result
        sessions.register_session(address, reference)
    }
    /// Method to insert a new session
    pub fn unregister_session(
        &mut self,
        session_type: SessionType,
        address: SocketAddr,
    ) -> SessionsResult<()> {
        // Get map to insert session to
        let sessions = self.get_sessions_by_type(session_type);

        // Remove session and return result
        sessions.unregister_session(address)
    }
    /// Method to insert a new session
    pub fn update_session(
        &mut self,
        session_type: SessionType,
        address: SocketAddr,
        session_status: SessionStatus,
    ) -> SessionsResult<()> {
        // Get map to insert session
        let sessions = self.get_sessions_by_type(session_type);

        // Update session and return result
        sessions.update_session(address, session_status)
    }
}
