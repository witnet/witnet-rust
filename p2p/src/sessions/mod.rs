//! Library for managing the node sessions, including outbound and inbound sessions

/// Bounded sessions module
pub mod bounded_sessions;

use std::net::SocketAddr;

use rand::{thread_rng, Rng};

use super::{error::SessionsError, sessions::bounded_sessions::BoundedSessions};
use crate::peers::get_range_address;
use std::collections::HashSet;

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
    /// Inbound consolidated sessions: __known__ peers sessions that connect to the server
    pub inbound_consolidated: BoundedSessions<T>,
    /// Keeps track of network ranges for inbound connections so as to prevent sybils from
    /// monopolizing our inbound capacity
    pub inbound_network_ranges: NetworkRangesCollection,
    /// Inbound sessions: __untrusted__ peers that connect to the server
    pub inbound_unconsolidated: BoundedSessions<T>,
    /// Magic number
    pub magic_number: u16,
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
    /// Server public address listening to incoming connections
    pub public_address: Option<SocketAddr>,
}

/// Default trait implementation
impl<T> Default for Sessions<T>
where
    T: Clone,
{
    fn default() -> Self {
        Self {
            inbound_consolidated: BoundedSessions::default(),
            inbound_network_ranges: Default::default(),
            inbound_unconsolidated: BoundedSessions::default(),
            magic_number: 0,
            outbound_consolidated: BoundedSessions::default(),
            outbound_consolidated_consensus: BoundedSessions::default(),
            outbound_unconsolidated: BoundedSessions::default(),
            public_address: None,
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
    ) -> Result<&mut BoundedSessions<T>, SessionsError> {
        match session_type {
            SessionType::Inbound => match status {
                SessionStatus::Unconsolidated => Ok(&mut self.inbound_unconsolidated),
                SessionStatus::Consolidated => Ok(&mut self.inbound_consolidated),
            },
            SessionType::Outbound => match status {
                SessionStatus::Unconsolidated => Ok(&mut self.outbound_unconsolidated),
                SessionStatus::Consolidated => Ok(&mut self.outbound_consolidated),
            },
            _ => Err(SessionsError::NotExpectedFeelerPeer),
        }
    }
    /// Method to set the server address
    pub fn set_public_address(&mut self, public_address: Option<SocketAddr>) {
        self.public_address = public_address;
    }
    /// Method to set the sessions limits
    pub fn set_limits(&mut self, inbound_limit: u16, outbound_consolidated_limit: u16) {
        self.inbound_consolidated.set_limit(inbound_limit);
        self.outbound_consolidated
            .set_limit(outbound_consolidated_limit);
        self.outbound_consolidated_consensus
            .set_limit(outbound_consolidated_limit);
    }
    /// Method to set the magic number to build messages
    pub fn set_magic_number(&mut self, magic_number: u16) {
        self.magic_number = magic_number;
    }
    /// Method to check if a socket address is eligible as outbound peer
    pub fn is_outbound_address_eligible(&self, candidate_addr: SocketAddr) -> bool {
        // Check if address is already used as consolidated outbound session
        if self
            .outbound_consolidated
            .collection
            .contains_key(&candidate_addr)
        {
            return false;
        }

        // Check if address is already used as unconsolidated outbound session
        if self
            .outbound_unconsolidated
            .collection
            .contains_key(&candidate_addr)
        {
            return false;
        }

        // Check if address is the server address
        if self
            .public_address
            .map(|address| address == candidate_addr)
            .unwrap_or(false)
        {
            return false;
        }

        // Return true if the address has not been used as outbound session or server address
        true
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
    /// Method to return the diff between limit and outbounds number
    pub fn num_missing_outbound(&self) -> usize {
        let num_outbound_sessions = self.get_num_outbound_sessions();

        self.outbound_consolidated
            .limit
            .map(|limit| usize::from(limit).saturating_sub(num_outbound_sessions))
            .unwrap_or(1)
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
    pub fn get_consolidated_inbound_sessions<'a>(&'a self) -> impl Iterator<Item = &T> + 'a {
        self.inbound_consolidated
            .collection
            .values()
            .map(|info| &info.reference)
    }

    /// Check whether a socket address is similar to that of any of the existing inbound sessions.
    pub fn is_similar_to_inbound_session(&self, addr: &SocketAddr) -> Option<&[u8; 4]> {
        self.inbound_network_ranges.contains_address(addr)
    }

    /// Method to insert a new session
    pub fn register_session(
        &mut self,
        session_type: SessionType,
        address: SocketAddr,
        reference: T,
    ) -> Result<(), SessionsError> {
        // Get map to insert session to
        let sessions = self.get_sessions(session_type, SessionStatus::Unconsolidated)?;

        // Register session and return result
        sessions.register_session(address, reference)?;

        // Register network range to prevent sybils from monopolizing our inbound capacity
        if session_type == SessionType::Inbound {
            self.inbound_network_ranges.insert_address(&address);
        }

        Ok(())
    }
    /// Method to remove a session
    /// Note: this does not close the socket, the connection will still be alive unless the actor
    /// is also stopped.
    pub fn unregister_session(
        &mut self,
        session_type: SessionType,
        status: SessionStatus,
        address: SocketAddr,
    ) -> Result<(), SessionsError> {
        // If this is an outbound consolidated session, try to remove it from the consensus list
        if let (SessionType::Outbound, SessionStatus::Consolidated) = (session_type, status) {
            // Explicitly ignore the result because we have no guarantees that this session was
            // inside the consensus map
            let _ = self.unconsensus_session(address);
        }

        // Get map to insert session to
        let sessions = self.get_sessions(session_type, status)?;

        // Remove session and return result
        sessions.unregister_session(address)?;

        // Unegister network range to allow other peers in same network range as the one we are
        // removing to take its place
        if session_type == SessionType::Inbound {
            self.inbound_network_ranges.remove_address(&address);
        }

        Ok(())
    }
    /// Method to consolidate a session
    pub fn consolidate_session(
        &mut self,
        session_type: SessionType,
        address: SocketAddr,
    ) -> Result<(), SessionsError> {
        // Get map to remove session from
        let uncons_sessions = self.get_sessions(session_type, SessionStatus::Unconsolidated)?;

        // Remove session from unconsolidated collection
        let session_info = uncons_sessions.unregister_session(address)?;

        // Get map to insert session to
        let cons_sessions = self.get_sessions(session_type, SessionStatus::Consolidated)?;

        // Register session into consolidated collection
        cons_sessions.register_session(address, session_info.reference)?;

        Ok(())
    }
    /// Method to mark a session as consensus safe
    pub fn consensus_session(&mut self, address: SocketAddr) -> Result<(), SessionsError> {
        if let Some(session_info) = self.outbound_consolidated.collection.get(&address) {
            let session_info = session_info.reference.clone();
            // Get map to insert session to
            let cons_sessions = &mut self.outbound_consolidated_consensus;
            // Register session into consolidated collection
            cons_sessions.register_session(address, session_info)
        } else {
            Err(SessionsError::NotOutboundConsolidatedPeer)
        }
    }
    /// Method to mark a session as consensus unsafe
    pub fn unconsensus_session(&mut self, address: SocketAddr) -> Result<(), SessionsError> {
        // Get map to remove session from
        let cons_sessions = &mut self.outbound_consolidated_consensus;

        // Remove session from unconsolidated collection
        cons_sessions.unregister_session(address).map(|_| ())
    }

    /// Get all the consolidated sessions addresses
    pub fn get_consolidated_sessions_addr(&self) -> GetConsolidatedPeersResult {
        GetConsolidatedPeersResult {
            inbound: self
                .inbound_consolidated
                .collection
                .iter()
                .map(|(k, _v)| *k)
                .collect(),
            outbound: self
                .outbound_consolidated
                .collection
                .iter()
                .map(|(k, _v)| *k)
                .collect(),
        }
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

/// List of inbound and outbound peers
#[derive(Clone, Debug)]
pub struct GetConsolidatedPeersResult {
    /// List of inbound peers: these opened a connection to us.
    /// The address shown here is the inbound address, we cannot use it to connect to this peer.
    pub inbound: Vec<SocketAddr>,
    /// List of outbound peers: we opened the connection to these ones.
    /// The address shown here can be used to connect to this peers in the future.
    pub outbound: Vec<SocketAddr>,
}

/// A convenient wrapper around a collection of network ranges.
/// This enables efficient tracking of the inbound connections we have so as to prevent sybil
/// machines from monopolizing our inbound peers table.
#[derive(Default)]
pub struct NetworkRangesCollection {
    inner: HashSet<[u8; 4]>,
    range_limit: u8,
}

impl NetworkRangesCollection {
    /// Checks whether a range is present in the collection as derived from a socket address.
    pub fn contains_address(&self, address: &SocketAddr) -> Option<&[u8; 4]> {
        let range_vec = get_range_address(address, self.range_limit);
        let mut range = [0, 0, 0, 0];
        range[..4].copy_from_slice(&range_vec);

        self.contains_range(range)
    }

    /// Checks whether a explicit range is present in the collection.
    pub fn contains_range(&self, range: [u8; 4]) -> Option<&[u8; 4]> {
        self.inner.get(&range)
    }

    /// Insert a range into the collection as derived from a socket address.
    pub fn insert_address(&mut self, address: &SocketAddr) -> bool {
        let range_vec = get_range_address(address, self.range_limit);
        let mut range = [0, 0, 0, 0];
        range[..4].copy_from_slice(&range_vec);

        self.insert_range(range)
    }

    /// Insert a explicit range into the collection.
    pub fn insert_range(&mut self, range: [u8; 4]) -> bool {
        self.inner.insert(range)
    }

    /// Remove a range from the collection as derived from a socket address.
    pub fn remove_address(&mut self, address: &SocketAddr) -> bool {
        let range_vec = get_range_address(address, self.range_limit);
        let mut range = [0, 0, 0, 0];
        range[..4].copy_from_slice(&range_vec);

        self.remove_range(range)
    }

    /// Remove a explicit range from the collection.
    pub fn remove_range(&mut self, range: [u8; 4]) -> bool {
        self.inner.remove(&range)
    }

    /// Set range limit
    pub fn set_range_limit(&mut self, range_limit: u8) {
        self.range_limit = range_limit;
    }
}

/// Compose a string for representing an IPV4 range
pub fn ip_range_string(range: &[u8], range_limit: u8) -> String {
    format!(
        "{}/{}",
        range.iter().fold(String::new(), |acc, &octet| acc
            + octet.to_string().as_str()
            + "."),
        range_limit,
    )
}
