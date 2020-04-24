//! Library for managing a list of available peers

use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use std::{cmp, collections::HashMap, fmt, net::SocketAddr};

use rand::seq::IteratorRandom;
use witnet_crypto::hash::calculate_sha256;
use witnet_util::timestamp::get_timestamp;

/// Peer information being used while listing available Witnet peers
#[derive(Serialize, Deserialize)]
struct PeerInfo {
    address: SocketAddr,
    timestamp: i64,
}

/// Peers TBD
#[derive(Default, Serialize, Deserialize)]
pub struct Peers {
    /// Bucket for tried addresses
    tried_bucket: HashMap<u16, PeerInfo>,
    /// Bucket for new addresses
    new_bucket: HashMap<u16, PeerInfo>,
    /// Nonce value
    sk: u64,
    /// Server SocketAddress
    server_address: Option<SocketAddr>,
}

impl Peers {
    /// Create a new instance of Peers
    pub fn new() -> Self {
        Peers {
            sk: thread_rng().gen(),
            tried_bucket: HashMap::new(),
            new_bucket: HashMap::new(),
            server_address: None,
        }
    }

    /// Set server address
    pub fn set_server(&mut self, server: SocketAddr) {
        self.server_address = Some(server);
    }

    /// Algorithm to calculate index for the new addresses buckets
    pub fn new_bucket_index(&self, socket_addr: &SocketAddr, src_socket_addr: &SocketAddr) -> u16 {
        let (_, group, host_id) = split_socket_addresses(socket_addr);
        let (_, src_group, _) = split_socket_addresses(src_socket_addr);

        calculate_index_for_new(self.sk, &src_group, &group, &host_id)
    }

    /// Algorithm to calculate index for the tried addresses buckets
    pub fn tried_bucket_index(&self, socket_addr: &SocketAddr) -> u16 {
        let (ip, group, host_id) = split_socket_addresses(socket_addr);

        calculate_index_for_tried(self.sk, &ip, &group, &host_id)
    }

    /// Contains for new bucket
    pub fn new_bucket_contains(&self, index: u16) -> bool {
        self.new_bucket.contains_key(&index)
    }

    /// Contains for tried bucket
    pub fn tried_bucket_contains(&self, index: u16) -> bool {
        self.tried_bucket.contains_key(&index)
    }

    /// Returns the timestamp of a specific slot in the new addresses bucket
    pub fn new_bucket_get_timestamp(&self, index: u16) -> Option<i64> {
        self.new_bucket.get(&index).map(|p| p.timestamp)
    }

    /// Returns the timestamp of a specific slot in the tried addresses bucket
    pub fn tried_bucket_get_timestamp(&self, index: u16) -> Option<i64> {
        self.tried_bucket.get(&index).map(|p| p.timestamp)
    }

    /// Returns the timestamp of a specific slot in the new addresses bucket
    pub fn new_bucket_get_address(&self, index: u16) -> Option<SocketAddr> {
        self.new_bucket.get(&index).map(|p| p.address)
    }

    /// Returns the timestamp of a specific slot in the tried addresses bucket
    pub fn tried_bucket_get_address(&self, index: u16) -> Option<SocketAddr> {
        self.tried_bucket.get(&index).map(|p| p.address)
    }

    /// Returns true if the address is the server address
    pub fn is_server_address(&self, addr: &SocketAddr) -> Option<bool> {
        if let Some(server) = self.server_address {
            Some(server == *addr)
        } else {
            None
        }
    }

    /// Add multiple peer addresses and save timestamp in the new addresses bucket
    /// If an address did already exist, it gets overwritten
    /// Returns all the overwritten addresses
    pub fn add_to_new(
        &mut self,
        addrs: Vec<SocketAddr>,
        src_address: SocketAddr,
    ) -> Result<Vec<SocketAddr>, failure::Error> {
        // Insert address
        // Note: if the peer address exists, the peer info will be overwritten
        let result = addrs
            .into_iter()
            .filter_map(|address| {
                // Filter out unspecified addresses (aka 0.0.0.0), and the server address
                if !address.ip().is_unspecified()
                    && !self.is_server_address(&address).unwrap_or(true)
                {
                    let index = self.tried_bucket_index(&address);
                    let elem = self.tried_bucket.get(&index);

                    // If the index point to the same address that it is already
                    // in tried, we don't include in new bucket
                    if elem.is_none() || (elem.unwrap().address != address) {
                        let index = self.new_bucket_index(&address, &src_address);

                        self.new_bucket
                            .insert(
                                index,
                                PeerInfo {
                                    address,
                                    timestamp: get_timestamp(), //msg.timestamp,
                                },
                            )
                            .map(|v| v.address)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        log::trace!("Added new peers: \n{}", self);

        Ok(result)
    }

    /// Add multiple peer addresses and save timestamp in the tried addresses bucket
    /// If an address did already exist, it gets overwritten
    /// Returns all the overwritten or rejected addresses
    pub fn add_to_tried(
        &mut self,
        address: SocketAddr,
    ) -> Result<Option<SocketAddr>, failure::Error> {
        // Insert address
        let result = if !address.ip().is_unspecified() {
            let index = self.tried_bucket_index(&address);

            self.tried_bucket
                .insert(
                    index,
                    PeerInfo {
                        address,
                        timestamp: get_timestamp(), //msg.timestamp,
                    },
                )
                .map(|v| v.address)
        } else {
            None
        };

        log::trace!("Added a tried peer: \n{}", self);

        Ok(result)
    }

    /// Remove a peer given an address from tried addresses bucket
    /// Returns the removed addresses
    pub fn remove_from_tried(&mut self, addrs: &[SocketAddr]) -> Vec<SocketAddr> {
        let v = addrs
            .iter()
            .filter_map(|address| {
                let index = self.tried_bucket_index(&address);
                let elem = self.tried_bucket.get(&index);

                if elem.is_some() && (elem.unwrap().address == *address) {
                    self.tried_bucket.remove(&index)
                } else {
                    None
                }
            })
            .map(|info| info.address)
            .collect();

        log::trace!("Removed a tried peer: \n{}", self);

        v
    }

    /// Remove a peer given an index from new addresses bucket
    /// Returns the removed addresses
    pub fn remove_from_new_with_index(&mut self, indexes: &[u16]) -> Vec<SocketAddr> {
        let v = indexes
            .iter()
            .filter_map(|index| self.new_bucket.remove(&index))
            .map(|info| info.address)
            .collect();

        log::trace!("Removed new peers: \n{}", self);

        v
    }

    /// Get a random socket address from the peers list
    /// This method provides the same probability to tried and new bucket peers
    pub fn get_random_peers(&self, n: usize) -> Result<Vec<SocketAddr>, failure::Error> {
        let mut rng = rand::thread_rng();

        let tried_len = self.tried_bucket.len();
        let new_len = self.new_bucket.len();

        // Upper limit for this method is the sum of the two buckets length
        let n_peers = cmp::min(tried_len + new_len, n);

        // In case of 0 peers required, returns an empty vector
        let mut v_peers: Vec<SocketAddr> = vec![];
        if n_peers == 0 {
            return Ok(v_peers);
        }
        // In case of not enough tried peers to complete the request
        // A minimum of new peers is required
        let min_new_required = n_peers.saturating_sub(tried_len);

        // Run n experiments with probability of success 50% to obtain
        // the peers number required from the new bucket
        let index_new_peers = (0..n_peers).fold(0, |acc, _| acc + rng.gen_range(0, 1));
        // Apply upper and lower limits to index_new_peers
        let index_new_peers = match index_new_peers {
            x if x < min_new_required => min_new_required,
            x if x > new_len => new_len,
            x => x,
        };

        // Obtains random peers from each bucket
        v_peers.extend(
            self.new_bucket
                .values()
                .map(|p| p.address)
                .choose_multiple(&mut rng, index_new_peers),
        );
        v_peers.extend(
            self.tried_bucket
                .values()
                .map(|p| p.address)
                .choose_multiple(&mut rng, n_peers - index_new_peers),
        );

        Ok(v_peers)
    }

    /// Get a random socket address from the new peers list
    pub fn get_new_random_peer(&self) -> Option<(u16, SocketAddr)> {
        let mut rng = rand::thread_rng();
        self.new_bucket
            .iter()
            .choose(&mut rng)
            .map(|(k, v)| (*k, v.address))
    }

    /// Get all the peers from the tried bucket
    pub fn get_all_from_tried(&self) -> Result<Vec<SocketAddr>, failure::Error> {
        Ok(self.tried_bucket.values().map(|v| v.address).collect())
    }

    /// Get all the peers from the tried bucket
    pub fn get_all_from_new(&self) -> Result<Vec<SocketAddr>, failure::Error> {
        Ok(self.new_bucket.values().map(|v| v.address).collect())
    }

    /// Clear tried addresses bucket
    pub fn clear_tried_bucket(&mut self) {
        self.tried_bucket.clear();

        log::trace!("Cleared tried bucket: \n{}", self);
    }

    /// Clear new addresses bucket
    pub fn clear_new_bucket(&mut self) {
        self.new_bucket.clear();

        log::trace!("Cleared new bucket: \n{}", self);
    }
}

impl fmt::Display for Peers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        writeln!(f, "--------------")?;
        writeln!(f, "New Peers List")?;
        writeln!(f, "--------------")?;

        for p in self.new_bucket.values() {
            writeln!(f, "> {}", p.address)?;
        }

        writeln!(f, "----------------")?;
        writeln!(f, "Tried Peers List")?;
        writeln!(f, "----------------")?;

        for p in self.tried_bucket.values() {
            writeln!(f, "> {}", p.address)?;
        }
        writeln!(f)
    }
}

/// Returns the ip and ip split
fn split_socket_addresses(socket_addr: &SocketAddr) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    match socket_addr {
        SocketAddr::V4(addr) => {
            let ip = addr.ip().octets();
            let [port_a, port_b] = addr.port().to_be_bytes();
            let (left, right) = ip.split_at(ip.len() / 2);
            let data = [right, &[port_a], &[port_b]].concat();
            (ip.to_vec(), left.to_vec(), data)
        }
        SocketAddr::V6(addr) => {
            let ip = addr.ip().octets();
            let [port_a, port_b] = addr.port().to_be_bytes();
            let (left, right) = ip.split_at(ip.len() / 2);
            let data = [right, &[port_a], &[port_b]].concat();
            (ip.to_vec(), left.to_vec(), data)
        }
    }
}

/// Algorithm to calculate index for the tried addresses buckets
/// SK = random value chosen when node is born.
/// IP = the peer’s IP address and port number.
/// Group = the peer’s group
/// Host_ID = the peer's host id
///
/// i = Hash( SK, IP ) % 4
/// Bucket = Hash( SK, Group, i ) % 64
/// Slot = Hash( SK, Host_ID, i ) % 64
///
/// Index = Bucket * Slot
fn calculate_index_for_tried(sk: u64, ip: &[u8], group: &[u8], host_id: &[u8]) -> u16 {
    let sk = sk.to_be_bytes();

    let data = [&sk, ip].concat();
    let data_hash = calculate_sha256(&data);
    let i = data_hash.0[31] % 4;

    let data = [&sk, group, &[i]].concat();
    let data_hash = calculate_sha256(&data);
    let bucket = u16::from(data_hash.0[31]) % 64;

    let data = [&sk, host_id, &[i]].concat();
    let data_hash = calculate_sha256(&data);
    let slot = u16::from(data_hash.0[31]) % 64;

    (bucket * 64) + slot
}

/// Algorithm to calculate index for the new addresses buckets
/// SK = random value chosen when node is born.
/// IP = the peer’s IP address and port number.
/// Group = the peer’s group
/// Src_group = the source peer's group
///
/// i = Hash( SK, Src_group, Group ) % 32
/// Bucket = Hash( SK, Src_group, i ) % 256
/// Slot = Hash( SK, Host_ID, i ) % 64
///
/// Index = Bucket * Slot
fn calculate_index_for_new(sk: u64, src_group: &[u8], group: &[u8], host_id: &[u8]) -> u16 {
    let sk = sk.to_be_bytes();

    let data = [&sk, src_group, group].concat();
    let data_hash = calculate_sha256(&data);
    let i = data_hash.0[31] % 32;

    let data = [&sk, src_group, &[i]].concat();
    let data_hash = calculate_sha256(&data);
    let bucket = u16::from(data_hash.0[31]) % 256;

    let data = [&sk, host_id, &[i]].concat();
    let data_hash = calculate_sha256(&data);
    let slot = u16::from(data_hash.0[31]) % 64;

    (bucket * 64) + slot
}
