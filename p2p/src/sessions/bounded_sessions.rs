//! Library for managing the sessions
use std::collections::HashMap;
use std::net::SocketAddr;

use crate::sessions::error::{SessionsError, SessionsErrorKind, SessionsResult};
use witnet_util::error::WitnetError;

/// Session info
pub struct SessionInfo<T> {
    /// Session reference (e.g. actor address)
    pub reference: T,
}

/// Sessions struct contains:
/// - lists of sessions parametrized with their reference (T)
/// - sessions limit to be connected
pub struct BoundedSessions<T> {
    /// Collection of sessions
    pub collection: HashMap<SocketAddr, SessionInfo<T>>,
    /// Sessions limit
    pub limit: Option<u16>,
}

/// Default trait implementation
impl<T> Default for BoundedSessions<T> {
    fn default() -> Self {
        Self {
            collection: HashMap::new(),
            limit: None,
        }
    }
}

impl<T> BoundedSessions<T> {
    /// Method to set the sessions limits
    pub fn set_limit(&mut self, limit: u16) {
        self.limit = Some(limit);
    }
    /// Method to insert a new session
    pub fn register_session(&mut self, address: SocketAddr, reference: T) -> SessionsResult<()> {
        // Check num peers
        if self
            .limit
            .map(|limit| self.collection.len() >= limit as usize)
            .unwrap_or(false)
        {
            return Err(WitnetError::from(SessionsError::new(
                SessionsErrorKind::Register,
                address.to_string(),
                "Max number of peers reached".to_string(),
            )));
        }
        // Check if address is already in sessions collection
        if self.collection.contains_key(&address) {
            return Err(WitnetError::from(SessionsError::new(
                SessionsErrorKind::Register,
                address.to_string(),
                "Address already registered in sessions".to_string(),
            )));
        }
        // Insert session into the right collection
        self.collection.insert(address, SessionInfo { reference });

        // Return success
        Ok(())
    }
    /// Method to insert a new session
    pub fn unregister_session(&mut self, address: SocketAddr) -> SessionsResult<SessionInfo<T>> {
        // Insert session into the right map (if not present)
        match self.collection.remove(&address) {
            Some(info) => Ok(info),
            None => Err(WitnetError::from(SessionsError::new(
                SessionsErrorKind::Unregister,
                address.to_string(),
                "Address could not be unregistered (not found in sessions)".to_string(),
            ))),
        }
    }
}
