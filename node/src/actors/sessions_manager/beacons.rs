use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use witnet_data_structures::chain::CheckpointBeacon;

/// Stores the CheckpointBeacons received from our peers, and also keeps track
/// of the list of peers which have not sent us a beacon yet.
/// The logic is simple: on every new epoch wait until we have beacons from all
/// the peers, and then send a PeersBeacons message to ChainManager.
/// The message-sending logic is implemented in SessionsManager.
#[derive(Default)]
pub struct Beacons {
    // Have we already sent a PeersBeacons message to ChainManager during this epoch?
    beacons_already_sent: bool,
    // Peers which have not sent us their beacon yet and we are waiting for them
    peers_not_beacon: HashSet<SocketAddr>,
    // Peers which have already sent us their beacon
    peers_with_beacon: HashMap<SocketAddr, CheckpointBeacon>,
}

impl Beacons {
    /// Have we already sent a PeersBeacons message during this epoch?
    pub fn already_sent(&self) -> bool {
        self.beacons_already_sent
    }

    /// Have all the peers sent us a beacon since the last call to clear()?
    pub fn all(&self) -> bool {
        self.peers_not_beacon.is_empty()
    }

    /// Return number of peers which have sent us a beacon, or are expected to
    /// send it to us
    pub fn total_count(&self) -> usize {
        self.peers_with_beacon.len() + self.peers_not_beacon.len()
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
    /// Returns true if the element was inserted correctly, and false if it a
    /// beacon from this peer was not expected
    pub fn insert(&mut self, k: SocketAddr, v: CheckpointBeacon) -> bool {
        if self.peers_not_beacon.remove(&k) {
            // If we were waiting for a beacon from this peer, remove it from
            // peers_not_beacon and insert it to peers_with_beacon
            self.peers_with_beacon.insert(k, v);

            true
        } else if let Entry::Occupied(mut e) = self.peers_with_beacon.entry(k) {
            // If we already have a beacon from this peers, overwrite it
            // So if a peer sends us more than one beacon, we use the last one
            // Except if we already have sent the peers beacons message, then
            // we will just ignore this beacon
            e.insert(v);

            true
        } else {
            // We got an unexpected beacon

            false
        }
    }

    /// Get all the beacons in order to send a PeersBeacons message.
    /// Returns a tuple of (peers which have sent us beacons, peers which have not)
    /// or None if a PeersBeacons message was already sent during this epoch
    pub fn send(
        &mut self,
    ) -> Option<(&HashMap<SocketAddr, CheckpointBeacon>, &HashSet<SocketAddr>)> {
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
    use witnet_data_structures::chain::Hash;

    // Helper functions needed because using assert_eq! with hashmaps is non-ergonomic
    fn pb_to_sorted_vec<'a, 'b, I: IntoIterator<Item = (&'a SocketAddr, &'b CheckpointBeacon)>>(
        pb: I,
    ) -> Vec<(SocketAddr, CheckpointBeacon)> {
        pb.into_iter()
            .map(|(k, v)| (*k, *v))
            .sorted_by_key(|(k, _v)| k.to_string())
            .collect()
    }

    fn pnb_to_sorted_vec<'a, 'b, I: IntoIterator<Item = (&'a SocketAddr)>>(
        pb: I,
    ) -> Vec<(SocketAddr)> {
        pb.into_iter()
            .cloned()
            .sorted_by_key(|k| k.to_string())
            .collect()
    }

    #[test]
    fn empty() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: Hash::default(),
        };

        let mut b = Beacons::default();
        assert_eq!(b.already_sent(), false);
        // Since we are waiting for 0 beacons, b.all() returns true
        assert_eq!(b.all(), true);
        // Before calling clear for the first time, insert always returns false
        // because the list of peers is empty
        assert_eq!(b.insert(k0, va), false);
        assert_eq!(b.insert(k1, va), false);
        // So we can send an empty message
        let (pb, pnb) = b.send().unwrap();
        assert!(pb.is_empty());
        assert!(pnb.is_empty());
        assert_eq!(b.already_sent(), true);

        // Wait for two beacons
        b.clear([k0, k1].iter().cloned());
        assert_eq!(b.all(), false);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert_eq!(b.already_sent(), false);
        assert_eq!(b.all(), false);
        // Try to send before receiving any beacons
        let (pb, pnb) = b.send().unwrap();
        assert!(pb.is_empty());
        assert_eq!(pnb_to_sorted_vec(pnb), vec![k0, k1]);
    }

    #[test]
    fn one_peer() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: Hash::default(),
        };
        let vb = CheckpointBeacon {
            checkpoint: 1,
            hash_prev_block: Hash::default(),
        };

        let mut b = Beacons::default();
        // Test case with only one peer
        b.clear([k0].iter().cloned());
        assert_eq!(b.all(), false);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert_eq!(b.already_sent(), false);
        assert_eq!(b.all(), false);
        assert_eq!(b.insert(k0, va), true);
        assert_eq!(b.all(), true);
        assert_eq!(b.insert(k1, va), false);
        assert_eq!(b.all(), true);
        // Inserting again also returns true, and the new beacon overwrites the old one
        assert_eq!(b.insert(k0, vb), true);
        assert_eq!(b.all(), true);
        assert_eq!(b.insert(k1, vb), false);
        assert_eq!(b.all(), true);

        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb_to_sorted_vec(pb), vec![(k0, vb)]);
        assert!(pnb.is_empty());
        assert_eq!(b.already_sent(), true);
    }

    #[test]
    fn two_peers() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: Hash::default(),
        };
        let vb = CheckpointBeacon {
            checkpoint: 1,
            hash_prev_block: Hash::default(),
        };

        let mut b = Beacons::default();
        // Test case with two peers
        b.clear([k0, k1].iter().cloned());
        assert_eq!(b.all(), false);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert_eq!(b.already_sent(), false);
        assert_eq!(b.all(), false);
        assert_eq!(b.insert(k0, va), true);
        assert_eq!(b.all(), false);
        assert_eq!(b.insert(k1, va), true);
        assert_eq!(b.all(), true);
        // Inserting again also returns true, and the new beacon overwrites the old one
        assert_eq!(b.insert(k0, vb), true);
        assert_eq!(b.all(), true);
        assert_eq!(b.insert(k1, vb), true);
        assert_eq!(b.all(), true);

        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb_to_sorted_vec(pb), vec![(k0, vb), (k1, vb)]);
        assert!(pnb.is_empty());
        assert_eq!(b.already_sent(), true);
    }

    #[test]
    fn two_peers_one_before_epoch() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: Hash::default(),
        };

        let mut b = Beacons::default();
        // Test case with two peers, one before new_epoch
        b.clear([k0, k1].iter().cloned());
        assert_eq!(b.all(), false);
        assert_eq!(b.insert(k0, va), true);
        assert_eq!(b.all(), false);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert_eq!(b.already_sent(), false);
        assert_eq!(b.all(), false);
        assert_eq!(b.insert(k1, va), true);
        assert_eq!(b.all(), true);

        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb_to_sorted_vec(pb), vec![(k0, va), (k1, va)]);
        assert!(pnb.is_empty());
        assert_eq!(b.already_sent(), true);
    }

    #[test]
    fn two_peers_one_no_beacon() {
        let k0 = "127.0.0.1:1110".parse().unwrap();
        let k1 = "127.0.0.1:1111".parse().unwrap();
        let va = CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: Hash::default(),
        };

        let mut b = Beacons::default();
        // Test case with two peers, one doesnt send beacon
        b.clear([k0, k1].iter().cloned());
        assert_eq!(b.all(), false);
        // The already_sent flag is cleared on new epoch
        b.new_epoch();
        assert_eq!(b.already_sent(), false);
        assert_eq!(b.all(), false);
        assert_eq!(b.insert(k0, va), true);
        assert_eq!(b.all(), false);

        let (pb, pnb) = b.send().unwrap();
        assert_eq!(pb_to_sorted_vec(pb), vec![(k0, va)]);
        assert_eq!(pnb_to_sorted_vec(pnb), vec![k1]);
        assert_eq!(b.already_sent(), true);

        assert_eq!(b.insert(k1, va), true);
        assert_eq!(b.all(), true);
        // But if we try to send now, it fails
        assert_eq!(b.send(), None);
    }
}
