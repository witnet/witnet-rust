use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use witnet_data_structures::types::LastBeacon;

/// Stores the LastBeacons received from our peers, and also keeps track
/// of the list of peers which have not sent us a beacon yet.
/// The logic is simple: on every new epoch wait until we have as many beacons
/// as outbound peers, and then send a PeersBeacons message to ChainManager.
/// The message-sending logic is implemented in SessionsManager.
#[derive(Default)]
pub struct Beacons {
    // Have we already sent a PeersBeacons message to ChainManager during this epoch?
    beacons_already_sent: bool,
    // Peers which have not sent us their beacon yet
    // These will be marked as out of consensus and dropped if they do not send a beacon in time
    peers_not_beacon: HashSet<SocketAddr>,
    // Peers which have already sent us their beacon
    peers_with_beacon: HashMap<SocketAddr, LastBeacon>,
}

impl Beacons {
    /// Have we already sent a PeersBeacons message during this epoch?
    pub fn already_sent(&self) -> bool {
        self.beacons_already_sent
    }

    /// Return number of peers which have sent us a beacon
    pub fn total_count(&self) -> usize {
        self.peers_with_beacon.len()
    }

    /// Clear the existing lists of peers and start waiting for the new ones
    pub fn clear<I: IntoIterator<Item = SocketAddr>>(&mut self, peers: I) {
        self.peers_not_beacon.clear();
        self.peers_with_beacon.clear();
        for socket_addr in peers {
            self.peers_not_beacon.insert(socket_addr);
        }
    }

    /// On new epoch we can send the PeersBeacons message again
    pub fn new_epoch(&mut self) {
        self.beacons_already_sent = false;
    }

    /// Insert a beacon. Overwrites already existing entries.
    pub fn insert(&mut self, k: SocketAddr, v: LastBeacon) {
        // Remove the peer from the waiting list
        // If we were not expecting a beacon from this peer, it doesn't matter,
        // act as if we had been expecting it
        self.peers_not_beacon.remove(&k);

        // If we already have a beacon from this peers, overwrite it
        // So if a peer sends us more than one beacon, we use the last one
        // Except if we already have sent the peers beacons message, then
        // we will just ignore this beacon
        self.peers_with_beacon.insert(k, v);
    }

    /// Remove beacon. Used when a peer disconnects before we reach consensus:
    /// we do not want to count that beacon
    pub fn remove(&mut self, k: &SocketAddr) {
        self.peers_not_beacon.remove(k);
        self.peers_with_beacon.remove(k);
    }

    /// When a new peer connects, we add it to the peers_not_beacon map, in order to
    /// close the connection if the peer is not in consensus
    pub fn also_wait_for(&mut self, k: SocketAddr) {
        if !self.peers_with_beacon.contains_key(&k) {
            self.peers_not_beacon.insert(k);
        }
    }

    /// Get all the beacons in order to send a PeersBeacons message.
    /// Returns a tuple of (peers which have sent us beacons, peers which have not)
    /// or None if a PeersBeacons message was already sent during this epoch
    pub fn send(&mut self) -> Option<(&HashMap<SocketAddr, LastBeacon>, &HashSet<SocketAddr>)> {
        if !self.beacons_already_sent {
            self.beacons_already_sent = true;

            Some((&self.peers_with_beacon, &self.peers_not_beacon))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;
    use witnet_data_structures::chain::{CheckpointBeacon, Hash};

    // Helper functions needed because using assert_eq! with hashmaps is non-ergonomic
    fn pb_to_sorted_vec<'a, 'b, I: IntoIterator<Item = (&'a SocketAddr, &'b LastBeacon)>>(
        pb: I,
    ) -> Vec<(SocketAddr, LastBeacon)> {
        pb.into_iter()
            .map(|(k, v)| (*k, v.clone()))
            .sorted_by_key(|(k, _v)| k.to_string())
            .collect()
    }

    fn pnb_to_sorted_vec<'a, 'b, I: IntoIterator<Item = &'a SocketAddr>>(pb: I) -> Vec<SocketAddr> {
        pb.into_iter()
            .cloned()
            .sorted_by_key(|k| k.to_string())
            .collect()
    }

    // Create a beacon set to default hash and the given checkpoint
    fn beacon(checkpoint: u32) -> LastBeacon {
        LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint,
                hash_prev_block: Hash::default(),
            },
            highest_superblock_checkpoint: CheckpointBeacon::default(),
        }
    }

    #[test]
    fn empty() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = beacon(0);

        let mut b = Beacons::default();
        assert!(!b.already_sent());
        // Since we are waiting for 0 beacons
        assert_eq!(b.total_count(), 0);
        // Before calling clear for the first time, insert always accepts new beacons
        // And no peers are penalized
        b.insert(k0, va.clone());
        b.insert(k1, va);
        // So we can send an empty message
        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb.len(), 2);
        assert!(pnb.is_empty());
        assert!(b.already_sent());

        // Wait for two beacons
        b.clear([k0, k1].iter().cloned());
        assert_eq!(b.total_count(), 0);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert!(!b.already_sent());
        assert_eq!(b.total_count(), 0);
        // Try to send before receiving any beacons
        let (pb, pnb) = b.send().unwrap();
        assert!(pb.is_empty());
        assert_eq!(pnb_to_sorted_vec(pnb), vec![k0, k1]);
    }

    #[test]
    fn one_peer() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = beacon(0);
        let vb = beacon(1);

        let mut b = Beacons::default();
        // Test case with only one peer excepted
        b.clear([k0].iter().cloned());
        assert_eq!(b.total_count(), 0);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert!(!b.already_sent());
        assert_eq!(b.total_count(), 0);
        b.insert(k0, va.clone());
        assert_eq!(b.total_count(), 1);
        b.insert(k1, va);
        assert_eq!(b.total_count(), 2);
        // Inserting again, the new beacon overwrites the old one
        b.insert(k0, vb.clone());
        assert_eq!(b.total_count(), 2);
        b.insert(k1, vb.clone());
        assert_eq!(b.total_count(), 2);

        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb_to_sorted_vec(pb), vec![(k0, vb.clone()), (k1, vb)]);
        assert!(pnb.is_empty());
        assert!(b.already_sent());
    }

    #[test]
    fn two_peers() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = beacon(0);
        let vb = beacon(1);

        let mut b = Beacons::default();
        // Test case with two peers
        b.clear([k0, k1].iter().cloned());
        assert_eq!(b.total_count(), 0);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert!(!b.already_sent());
        assert_eq!(b.total_count(), 0);
        b.insert(k0, va.clone());
        assert_eq!(b.total_count(), 1);
        b.insert(k1, va);
        assert_eq!(b.total_count(), 2);
        // Inserting again, the new beacon overwrites the old one
        b.insert(k0, vb.clone());
        assert_eq!(b.total_count(), 2);
        b.insert(k1, vb.clone());
        assert_eq!(b.total_count(), 2);

        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb_to_sorted_vec(pb), vec![(k0, vb.clone()), (k1, vb)]);
        assert!(pnb.is_empty());
        assert!(b.already_sent());
    }

    #[test]
    fn two_peers_one_before_epoch() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = beacon(0);

        let mut b = Beacons::default();
        // Test case with two peers, one before new_epoch
        b.clear([k0, k1].iter().cloned());
        assert_eq!(b.total_count(), 0);
        b.insert(k0, va.clone());
        assert_eq!(b.total_count(), 1);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert!(!b.already_sent());
        // But the beacons are only cleared when calling .clear()
        assert_eq!(b.total_count(), 1);
        b.insert(k1, va.clone());
        assert_eq!(b.total_count(), 2);

        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb_to_sorted_vec(pb), vec![(k0, va.clone()), (k1, va)]);
        assert!(pnb.is_empty());
        assert!(b.already_sent());
    }

    #[test]
    fn two_peers_one_no_beacon() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = beacon(0);

        let mut b = Beacons::default();
        // Test case with two peers, one doesnt send beacon
        b.clear([k0, k1].iter().cloned());
        assert_eq!(b.total_count(), 0);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert!(!b.already_sent());
        assert_eq!(b.total_count(), 0);
        b.insert(k0, va.clone());
        assert_eq!(b.total_count(), 1);

        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb_to_sorted_vec(pb), vec![(k0, va.clone())]);
        assert_eq!(pnb_to_sorted_vec(pnb), vec![k1]);
        assert!(b.already_sent());

        b.insert(k1, va);
        assert_eq!(b.total_count(), 2);
        // But if we try to send now, it fails because it was already sent
        assert_eq!(b.send(), None);
    }

    #[test]
    fn two_peers_one_disconnect() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = beacon(0);

        let mut b = Beacons::default();
        // Test case with two peers, one disconnects after sending beacon
        b.clear([k0, k1].iter().cloned());
        assert_eq!(b.total_count(), 0);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert!(!b.already_sent());
        assert_eq!(b.total_count(), 0);
        b.insert(k0, va.clone());
        assert_eq!(b.total_count(), 1);

        // Now first peer disconnects
        b.remove(&k0);
        assert_eq!(b.total_count(), 0);

        // And second peer send beacon
        b.insert(k1, va.clone());
        assert_eq!(b.total_count(), 1);

        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb_to_sorted_vec(pb), vec![(k1, va)]);
        // The first peer is not marked as "out of consensus" because it has already disconnected
        assert_eq!(pnb_to_sorted_vec(pnb), vec![]);
        assert!(b.already_sent());
    }

    #[test]
    fn one_peer_connects_later() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = beacon(0);

        let mut b = Beacons::default();
        // Test case with one peer connecting after the call to .clear()
        b.clear([k0].iter().cloned());
        assert_eq!(b.total_count(), 0);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert!(!b.already_sent());
        assert_eq!(b.total_count(), 0);
        b.insert(k0, va.clone());
        assert_eq!(b.total_count(), 1);

        // Now a new peer connects but doesn't send beacon
        b.also_wait_for(k1);
        assert_eq!(b.total_count(), 1);

        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb_to_sorted_vec(pb), vec![(k0, va)]);
        // The second peer is marked as "out of consensus" because it has not sent any beacon
        assert_eq!(pnb_to_sorted_vec(pnb), vec![k1]);
        assert!(b.already_sent());
    }
}
