//! Library for managing the node sessions, including outbound and inbound sessions

/// Bounded sessions module
pub mod bounded_sessions;

use std::{net::SocketAddr, time::Duration};

use rand::{thread_rng, Rng};

use super::{error::SessionsError, sessions::bounded_sessions::BoundedSessions};

/// Session type
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SessionType {
    /// Inbound session
    Inbound,
    /// Outbound session
    Outbound,
    /// Session created by feeler function
    Feeler,
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
/// - list of inbound sessions parametrized with their reference (T)
/// - list of consolidated outbound sessions parametrized with their reference(T)
/// - list of unconsolidated outbound sessions parametrized with their reference(T)
pub struct Sessions<T>
where
    T: Clone,
{
    /// Server address listening to incoming connections
    pub server_address: Option<SocketAddr>,
    /// Inbound consolidated sessions: __known__ peers sessions that connect to the server
    pub inbound_consolidated: BoundedSessions<T>,
    /// Inbound sessions: __untrusted__ peers that connect to the server
    pub inbound_unconsolidated: BoundedSessions<T>,
    /// Outbound consolidated sessions: __known__ peer sessions that the node is connected to (in
    /// consolidated status)
    pub outbound_consolidated: BoundedSessions<T>,
    /// Outbound consolidated sessions: __known__ peer sessions that the node is connected to, and
    /// are in consensus about what is the tip of the chain (their last beacon is the same as ours)
    /// Note: this is a subset of `outboud_consolidated`.
    pub outbound_consolidated_consensus: BoundedSessions<T>,
    /// Outbound unconsolidated sessions: __known__ peer sessions that the node is connected to
    /// (in unconsolidated status)
    pub outbound_unconsolidated: BoundedSessions<T>,
    /// Handshake timeout
    pub handshake_timeout: Duration,
    /// Magic number
    pub magic_number: u16,
    /// Timeout for requested blocks
    pub blocks_timeout: i64,
}

/// Default trait implementation
impl<T> Default for Sessions<T>
where
    T: Clone,
{
    fn default() -> Self {
        Self {
            server_address: None,
            inbound_consolidated: BoundedSessions::default(),
            inbound_unconsolidated: BoundedSessions::default(),
            outbound_consolidated: BoundedSessions::default(),
            outbound_consolidated_consensus: BoundedSessions::default(),
            outbound_unconsolidated: BoundedSessions::default(),
            handshake_timeout: Duration::default(),
            magic_number: 0 as u16,
            blocks_timeout: 0 as i64,
        }
    }
}

impl<T> Sessions<T>
where
    T: Clone,
{
    /// Method to get the sessions map by a session type
    fn get_sessions(
        &mut self,
        session_type: SessionType,
        status: SessionStatus,
    ) -> Result<&mut BoundedSessions<T>, failure::Error> {
        match session_type {
            SessionType::Inbound => match status {
                SessionStatus::Unconsolidated => Ok(&mut self.inbound_unconsolidated),
                SessionStatus::Consolidated => Ok(&mut self.inbound_consolidated),
            },
            SessionType::Outbound => match status {
                SessionStatus::Unconsolidated => Ok(&mut self.outbound_unconsolidated),
                SessionStatus::Consolidated => Ok(&mut self.outbound_consolidated),
            },
            _ => Err(SessionsError::NotExpectedFeelerPeer)?,
        }
    }
    /// Method to set the server address
    pub fn set_server_address(&mut self, server_address: SocketAddr) {
        self.server_address = Some(server_address);
    }
    /// Method to set the sessions limits
    pub fn set_limits(&mut self, inbound_limit: u16, outbound_consolidated_limit: u16) {
        self.inbound_consolidated.set_limit(inbound_limit);
        self.outbound_consolidated
            .set_limit(outbound_consolidated_limit);
        self.outbound_consolidated_consensus
            .set_limit(outbound_consolidated_limit);
    }
    /// Method to set the handshake timeout
    pub fn set_handshake_timeout(&mut self, handshake_timeout: Duration) {
        self.handshake_timeout = handshake_timeout;
    }
    /// Method to set the magic number to build messages
    pub fn set_magic_number(&mut self, magic_number: u16) {
        self.magic_number = magic_number;
    }
    /// Method to set the timout for waiting requested blocks
    pub fn set_blocks_timeout(&mut self, blocks_timeout: i64) {
        self.blocks_timeout = blocks_timeout;
    }
    /// Method to check if a socket address is eligible as outbound peer
    pub fn is_outbound_address_eligible(&self, candidate_addr: SocketAddr) -> bool {
        // Check if address is already used as outbound session (consolidated or unconsolidated)
        let is_outbound_consolidated = self
            .outbound_consolidated
            .collection
            .contains_key(&candidate_addr);
        let is_outbound_unconsolidated = self
            .outbound_unconsolidated
            .collection
            .contains_key(&candidate_addr);

        // Check if address is the server address
        let is_server = self
            .server_address
            .map(|address| address == candidate_addr)
            .unwrap_or(false);

        // Return true if the address has not been used as outbound session or server address
        !is_outbound_consolidated && !is_outbound_unconsolidated && !is_server
    }
    /// Method to get total number of outbound peers
    pub fn get_num_outbound_sessions(&self) -> usize {
        self.outbound_consolidated.collection.len() + self.outbound_unconsolidated.collection.len()
    }
    /// Method to get number of inbound peers
    pub fn get_num_inbound_sessions(&self) -> usize {
        self.inbound_consolidated.collection.len()
    }
    /// Method to check if outbound bootstrap is needed
    pub fn is_outbound_bootstrap_needed(&self) -> bool {
        let num_outbound_sessions = self.get_num_outbound_sessions();

        self.outbound_consolidated
            .limit
            .map(|limit| num_outbound_sessions < limit as usize)
            .unwrap_or(true)
    }
    /// Method to get a random consolidated outbound session
    pub fn get_random_anycast_session(&self, safu: bool) -> Option<T> {
        // Get iterator over the values of the hashmap
        let mut outbound_sessions_iter = if safu {
            // Safu: use only peers with consensus
            self.outbound_consolidated_consensus.collection.values()
        } else {
            // Not safu: use all peers
            self.outbound_consolidated.collection.values()
        };

        // Get the number of elements in the collection from the iterator
        let len = outbound_sessions_iter.len();

        // Get random index
        let index: usize = if len == 0 {
            0
        } else {
            thread_rng().gen_range(0, len)
        };

        // Get session info reference at random index (None if no elements in the collection)
        outbound_sessions_iter
            .nth(index)
            .map(|info| info.reference.clone())
    }
    /// Method to get all the consolidated sessions (inbound and outbound)
    pub fn get_all_consolidated_sessions<'a>(&'a self) -> impl Iterator<Item = &T> + 'a {
        self.outbound_consolidated
            .collection
            .values()
            .chain(self.inbound_consolidated.collection.values())
            .map(|info| &info.reference)
    }

    /// Method to get all the consolidated sessions (inbound and outbound)
    pub fn get_all_consolidated_inbound_sessions<'a>(&'a self) -> impl Iterator<Item = &T> + 'a {
        self.inbound_consolidated
            .collection
            .values()
            .map(|info| &info.reference)
    }

    /// Method to insert a new session
    pub fn register_session(
        &mut self,
        session_type: SessionType,
        address: SocketAddr,
        reference: T,
    ) -> Result<(), failure::Error> {
        // Get map to insert session to
        let sessions = self.get_sessions(session_type, SessionStatus::Unconsolidated)?;

        // Register session and return result
        sessions.register_session(address, reference)
    }
    /// Method to remove a session
    /// Note: this does not close the socket, the connection will still be alive unless the actor
    /// is also stopped.
    pub fn unregister_session(
        &mut self,
        session_type: SessionType,
        status: SessionStatus,
        address: SocketAddr,
    ) -> Result<(), failure::Error> {
        // If this is an outbound consolidated session, try to remove it from the consensus list
        if let (SessionType::Outbound, SessionStatus::Consolidated) = (session_type, status) {
            match self.unconsensus_session(address) {
                // Explicitly ignore the result because we have no guarantees that this session was
                // inside the consensus map
                _ => {}
            }
        }

        // Get map to insert session to
        let sessions = self.get_sessions(session_type, status)?;

        // Remove session and return result
        sessions.unregister_session(address).map(|_| ())
    }
    /// Method to consolidate a session
    pub fn consolidate_session(
        &mut self,
        session_type: SessionType,
        address: SocketAddr,
    ) -> Result<(), failure::Error> {
        // Get map to remove session from
        let uncons_sessions = self.get_sessions(session_type, SessionStatus::Unconsolidated)?;

        // Remove session from unconsolidated collection
        let session_info = uncons_sessions.unregister_session(address)?;

        // Get map to insert session to
        let cons_sessions = self.get_sessions(session_type, SessionStatus::Consolidated)?;

        // Register session into consolidated collection
        cons_sessions.register_session(address, session_info.reference)
    }
    /// Method to mark a session as consensus safe
    pub fn consensus_session(&mut self, address: SocketAddr) -> Result<(), failure::Error> {
        if let Some(session_info) = self.outbound_consolidated.collection.get(&address) {
            let session_info = session_info.reference.clone();
            // Get map to insert session to
            let cons_sessions = &mut self.outbound_consolidated_consensus;
            // Register session into consolidated collection
            cons_sessions.register_session(address, session_info)
        } else {
            Err(SessionsError::NotOutboundConsolidatedPeer)?
        }
    }
    /// Method to mark a session as consensus unsafe
    pub fn unconsensus_session(&mut self, address: SocketAddr) -> Result<(), failure::Error> {
        // Get map to remove session from
        let cons_sessions = &mut self.outbound_consolidated_consensus;

        // Remove session from unconsolidated collection
        cons_sessions.unregister_session(address).map(|_| ())
    }

    /// Show the addresses of all the sessions
    pub fn show_ips(&self) -> Vec<String> {
        ["Inbound Unconsolidated".to_string()]
            .iter()
            .cloned()
            .chain(
                self.inbound_unconsolidated
                    .collection
                    .keys()
                    .map(ToString::to_string),
            )
            .chain(std::iter::once("Inbound Consolidated".to_string()))
            .chain(
                self.inbound_consolidated
                    .collection
                    .keys()
                    .map(ToString::to_string),
            )
            .chain(std::iter::once("Outbound Unconsolidated".to_string()))
            .chain(
                self.outbound_unconsolidated
                    .collection
                    .keys()
                    .map(ToString::to_string),
            )
            .chain(std::iter::once("Outbound Consolidated".to_string()))
            .chain(
                self.outbound_consolidated
                    .collection
                    .keys()
                    .map(ToString::to_string),
            )
            .chain(std::iter::once(
                "Outbound Consolidated Consensus".to_string(),
            ))
            .chain(
                self.outbound_consolidated_consensus
                    .collection
                    .keys()
                    .map(ToString::to_string),
            )
            .collect()
    }
}
