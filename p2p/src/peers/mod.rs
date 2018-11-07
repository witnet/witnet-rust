//! Library for managing a list of available peers

use serde_derive::{Deserialize, Serialize};

use std::collections::HashMap;
use std::net::SocketAddr;

use rand::{thread_rng, Rng};

use witnet_util::timestamp::get_timestamp;

use crate::peers::error::PeersResult;

pub mod error;

/// Peer information being used while listing available Witnet peers
#[derive(Serialize, Deserialize)]
struct PeerInfo {
    address: SocketAddr,
    _timestamp: i64,
}

/// Peers TBD
#[derive(Default, Serialize, Deserialize)]
pub struct Peers {
    /// Server sessions
    peers: HashMap<SocketAddr, PeerInfo>,
}

impl Peers {
    /// Add multiple peer addresses and save timestamp
    /// If an address did already exist, it gets overwritten
    /// Returns all the overwritten addresses
    pub fn add(&mut self, addrs: Vec<SocketAddr>) -> PeersResult<Vec<SocketAddr>> {
        // Insert address
        // Note: if the peer address exists, the peer info will be overwritten
        Ok(addrs
            .into_iter()
            .filter_map(|address| {
                self.peers
                    .insert(
                        address,
                        PeerInfo {
                            address,
                            _timestamp: get_timestamp(), //msg.timestamp,
                        },
                    )
                    .map(|v| v.address)
            })
            .collect())
    }

    /// Remove a peer given an address
    /// Returns the removed addresses
    pub fn remove(&mut self, addrs: &[SocketAddr]) -> PeersResult<Vec<SocketAddr>> {
        Ok(addrs
            .iter()
            .filter_map(|address| self.peers.remove(&address).map(|info| info.address))
            .collect())
    }

    /// Get a random socket address from the peers list
    pub fn get_random(&mut self) -> PeersResult<Option<SocketAddr>> {
        // Random index with range [0, len) of the peers vector
        let index = thread_rng().gen_range(0, std::cmp::max(self.peers.len(), 1));

        // Get element at index
        let random_addr = self
            .peers
            // get peer infos
            .values()
            // enumerate them -> (indices, peer info)
            .enumerate()
            // filter by index and get address -> Iterator<Option<SocketAddr>>
            .filter_map(|(i, v)| if i == index { Some(v.address) } else { None })
            // Get first one, because
            .next()
            .map(|v| v.to_owned());

        Ok(random_addr)
    }

    /// Get all the peers from the list
    pub fn get_all(&self) -> PeersResult<Vec<SocketAddr>> {
        Ok(self.peers.values().map(|v| v.address).collect())
    }
}
