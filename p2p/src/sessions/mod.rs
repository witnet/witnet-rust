//! Library for managing the sessions
use std::collections::HashMap;
use std::net::SocketAddr;

use crate::sessions::error::{SessionsError, SessionsErrorKind, SessionsResult};
use witnet_util::error::WitnetError;

pub mod error;

type SessionsMap<T> = HashMap<SocketAddr, SessionInfo<T>>;

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

/// Session info
pub struct SessionInfo<T> {
    /// Session reference (e.g. actor address)
    pub reference: T,
    /// Session status
    pub status: SessionStatus,
}

/// Sessions
/// This struct contains:
/// - server address used to listen to incoming connections
/// - lists of of inbound and outbound sessions parametrized with their reference (T)
/// - sessions limit to be connected
pub struct Sessions<T> {
    // Server address listening to incoming connections
    pub server_address: Option<SocketAddr>,
    /// Inbound sessions: __untrusted__ peers that connect to the server
    pub inbound_sessions: SessionsMap<T>,
    /// Outbound sessions: __known__ peers that the node is connected
    pub outbound_sessions: SessionsMap<T>,
    // Inbound sessions limit
    pub inbound_limit: usize,
    // Outbound sessions limit
    pub outbound_limit: usize,
}

impl<T> Default for Sessions<T> {
    fn default() -> Self {
        Self {
            server_address: None,
            inbound_sessions: HashMap::new(),
            outbound_sessions: HashMap::new(),
            inbound_limit: 0,
            outbound_limit: 0,
        }
    }
}

impl<T> Sessions<T> {
    /// Method to set the server address
    pub fn set_server_address(&mut self, server_adress: SocketAddr) {
        self.server_address = Some(server_adress);
    }
    /// Method to set the sessions limits
    pub fn set_limits(&mut self, inbound_limit: usize, outbound_limit: usize) {
        self.inbound_limit = inbound_limit;
        self.outbound_limit = outbound_limit;
    }
    /// Method to check if a socket address is eligible as outbound peer
    pub fn is_outbound_address_eligible(&self, candidate_addr: SocketAddr) -> bool {
        // Check if address is already used as outbound session
        let is_outbound = self.outbound_sessions.contains_key(&candidate_addr);
        // Check if address is the server address
        let is_server =
            self.server_address.is_some() && self.server_address.unwrap() == candidate_addr;

        // Return true if the address has not been used as outbound session or server address
        !is_outbound && !is_server
    }
    /// Method to get the sessions map by a session type
    pub fn get_limit_by_type(&self, session_type: SessionType) -> usize {
        match session_type {
            SessionType::Inbound => self.inbound_limit,
            SessionType::Outbound => self.outbound_limit,
        }
    }
    /// Method to get the sessions map by a session type
    pub fn get_sessions_by_type(&mut self, session_type: SessionType) -> &mut SessionsMap<T> {
        match session_type {
            SessionType::Inbound => &mut self.inbound_sessions,
            SessionType::Outbound => &mut self.outbound_sessions,
        }
    }
    /// Method to insert a new session
    pub fn register_session(
        &mut self,
        session_type: SessionType,
        address: SocketAddr,
        reference: T,
    ) -> SessionsResult<()> {
        // Get sessions limits
        let limit = self.get_limit_by_type(session_type);
        // Get map to insert session to
        let sessions = self.get_sessions_by_type(session_type);
        // Check num peers
        if sessions.len() >= limit {
            return Err(WitnetError::from(SessionsError::new(
                SessionsErrorKind::Register,
                address.to_string(),
                "Max number of peers reached".to_string(),
            )));
        }
        // Check if address is already in session
        if sessions.contains_key(&address) {
            return Err(WitnetError::from(SessionsError::new(
                SessionsErrorKind::Register,
                address.to_string(),
                "Address already registered in session".to_string(),
            )));
        }
        // Insert session into the right map (if not present)
        sessions.insert(
            address,
            SessionInfo {
                reference,
                status: SessionStatus::Consolidated,
            },
        );

        // Return success
        Ok(())
    }
    /// Method to insert a new session
    pub fn unregister_session(
        &mut self,
        session_type: SessionType,
        address: SocketAddr,
    ) -> SessionsResult<()> {
        // Get map to insert session to
        let sessions = self.get_sessions_by_type(session_type);
        // Insert session into the right map (if not present)
        match sessions.remove(&address) {
            Some(_) => Ok(()),
            None => Err(WitnetError::from(SessionsError::new(
                SessionsErrorKind::Unregister,
                address.to_string(),
                "Address could not be unregistered (not found in session)".to_string(),
            ))),
        }
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
        // If session exists, then apply update and return success
        if let Some(session) = sessions.get_mut(&address) {
            session.status = session_status;
            return Ok(());
        }
        // If session does not exist, then return error
        Err(WitnetError::from(SessionsError::new(
            SessionsErrorKind::Update,
            address.to_string(),
            "Address could not be updated (not found in sessions)".to_string(),
        )))
    }
}
