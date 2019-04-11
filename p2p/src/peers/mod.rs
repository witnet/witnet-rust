//! Library for managing a list of available peers

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::net::SocketAddr;

use rand::{thread_rng, Rng};

use witnet_util::timestamp::get_timestamp;

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
    pub fn add(&mut self, addrs: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, failure::Error> {
        // Insert address
        // Note: if the peer address exists, the peer info will be overwritten
        Ok(addrs
            .into_iter()
            // Filter out unspecified addresses (aka 0.0.0.0)
            .filter(|address| !address.ip().is_unspecified())
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
    pub fn remove(&mut self, addrs: &[SocketAddr]) -> Result<Vec<SocketAddr>, failure::Error> {
        Ok(addrs
            .iter()
            .filter_map(|address| self.peers.remove(&address).map(|info| info.address))
            .collect())
    }

    /// Get a random socket address from the peers list
    pub fn get_random(&mut self) -> Result<Option<SocketAddr>, failure::Error> {
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
    pub fn get_all(&self) -> Result<Vec<SocketAddr>, failure::Error> {
        Ok(self.peers.values().map(|v| v.address).collect())
    }
}
