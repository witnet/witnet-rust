use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use witnet_data_structures::types::ProtocolVersion;

use failure::Fail;

#[derive(Debug, Fail)]
pub enum NotSettingProtocolVersions {
    #[fail(display = "Protocol versions were already set")]
    AlreadySet,
    #[fail(display = "Not setting protocol info because of lack of peers (still bootstrapping)")]
    BootstrapNeeded,
    #[fail(
        display = "Not setting protocol info because not all peers sent matching protocol info vectors"
    )]
    MismatchingProtocolVersions,
    #[fail(display = "Not setting protocol info because protocol info vector was empty")]
    NoProtocolVersions,
    #[fail(
        display = "Not setting protocol info because not enough peers sent their protocol info vectors"
    )]
    NotEnoughPeers,
}

/// Stores the Versions received from our peers, and also keeps track
/// of the list of peers which have not sent us their protocol versions yet.
/// The message-sending logic is implemented in SessionsManager.
#[derive(Default)]
pub struct Versions {
    // Have we already set the protocol versions?
    protocol_versions_already_set: bool,
    // Peers which have not sent us their protocol versions yet
    // These will be marked as out of consensus and dropped if they do not send a beacon in time
    peers_without_protocol_versions: HashSet<SocketAddr>,
    // Peers which have already sent us their protocol versions
    peers_with_protocol_versions: HashMap<SocketAddr, Vec<ProtocolVersion>>,
}

impl Versions {
    // Update protocol_versions_already_set to true
    pub fn protocol_versions_set(&mut self) {
        self.protocol_versions_already_set = true;
    }

    // Have we already processed the protocol versions?
    pub fn already_set(&self) -> bool {
        self.protocol_versions_already_set
    }

    /// Return number of peers which have sent us their protocol versions
    pub fn total_count(&self) -> usize {
        self.peers_with_protocol_versions.len()
    }

    /// Insert protocol info vector. Overwrites already existing entries.
    pub fn insert(&mut self, k: SocketAddr, pv: Vec<ProtocolVersion>) {
        // Remove the peer from the waiting list
        self.peers_without_protocol_versions.remove(&k);

        // When bootstrapping the node, we request a version message from our peers.
        // If we already processed and set protocol versions, we do not add new ones
        // but instead check if this peer has sent us valid protocol versions
        self.peers_with_protocol_versions.insert(k, pv);
    }

    /// Remove protocol versions when a peer disconnects before we reach consensus:
    /// we do not want to count it
    pub fn remove(&mut self, k: &SocketAddr) {
        self.peers_without_protocol_versions.remove(k);
        self.peers_with_protocol_versions.remove(k);
    }

    /// When a new peer connects, we add it to the peers_not_beacon map, in order to
    /// close the connection if the peer is not in consensus
    pub fn also_wait_for(&mut self, k: SocketAddr) {
        if !self.peers_with_protocol_versions.contains_key(&k) {
            self.peers_without_protocol_versions.insert(k);
        }
    }

    pub fn check_protocol_versions(
        &self,
    ) -> Result<Vec<ProtocolVersion>, NotSettingProtocolVersions> {
        let protocol_verions: Vec<_> = self.peers_with_protocol_versions.values().collect();

        let mut protocols = protocol_verions.into_iter();
        let first_protocol = match protocols.next() {
            Some(protocol) => protocol,
            None => return Err(NotSettingProtocolVersions::NoProtocolVersions),
        };
        if protocols.all(|protocol| protocol == first_protocol) {
            log::debug!("All received protocol version vectors match");

            Ok(first_protocol.to_vec())
        } else {
            log::debug!(
                "Received protocol version vectors do not match: {:?}",
                self.peers_with_protocol_versions
            );

            Err(NotSettingProtocolVersions::MismatchingProtocolVersions)
        }
    }
}
