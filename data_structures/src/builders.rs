use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::u32::MAX as U32_MAX;

use rand::{thread_rng, Rng};

use witnet_util::timestamp::get_timestamp;

use crate::{
    chain::{
        Block, BlockHeader, BlockTransactions, CheckpointBeacon, InventoryEntry, LeadershipProof,
    },
    error::BuildersError,
    transaction::Transaction,
    types::{
        Address, Command, GetPeers, InventoryAnnouncement, InventoryRequest, IpAddress, LastBeacon,
        Message, Peers, Ping, Pong, Verack, Version,
    },
};

////////////////////////////////////////////////////////////////////////////////////////
// PROTOCOL MESSAGES CONSTANTS
////////////////////////////////////////////////////////////////////////////////////////
/// Protocol version (used in handshake)
pub const PROTOCOL_VERSION: u32 = 0x0000_0001;

/// Capabilities
pub const CAPABILITIES: u64 = 0x0000_0000_0000_0001;

/// User agent
pub const USER_AGENT: &str = "full-node-desktop-edition";

/// Genesis block
pub const GENESIS: u64 = 0x0123_4567_89AB_CDEF;

////////////////////////////////////////////////////////////////////////////////////////
// BUILDER PUBLIC FUNCTIONS
////////////////////////////////////////////////////////////////////////////////////////
impl Message {
    /// Function to build Ping messages
    pub fn build_ping(magic: u16) -> Message {
        Message::build_message(
            magic,
            Command::Ping(Ping {
                nonce: random_nonce(),
            }),
        )
    }

    /// Function to build Pong messages
    pub fn build_pong(magic: u16, nonce: u64) -> Message {
        Message::build_message(magic, Command::Pong(Pong { nonce }))
    }

    /// Function to build GetPeers messages
    pub fn build_get_peers(magic: u16) -> Message {
        Message::build_message(magic, Command::GetPeers(GetPeers))
    }

    /// Function to build Peers messages
    pub fn build_peers(magic: u16, peers: &[SocketAddr]) -> Message {
        // Cast all peers to witnet's address struct
        let mut casted_peers = Vec::new();
        peers.iter().for_each(|peer| {
            casted_peers.push(to_address(*peer));
        });

        Message::build_message(
            magic,
            Command::Peers(Peers {
                peers: casted_peers,
            }),
        )
    }

    /// Function to build Version messages
    pub fn build_version(
        magic: u16,
        sender_addr: SocketAddr,
        receiver_addr: SocketAddr,
        last_epoch: u32,
    ) -> Message {
        Message::build_message(
            magic,
            Command::Version(Version {
                version: PROTOCOL_VERSION,
                timestamp: get_timestamp(),
                capabilities: CAPABILITIES,
                sender_address: to_address(sender_addr),
                receiver_address: to_address(receiver_addr),
                user_agent: USER_AGENT.to_string(),
                last_epoch,
                nonce: random_nonce(),
            }),
        )
    }

    /// Function to build Verack messages
    pub fn build_verack(magic: u16) -> Message {
        Message::build_message(magic, Command::Verack(Verack))
    }

    /// Function to build InventoryAnnouncement messages
    pub fn build_inventory_announcement(
        magic: u16,
        inv_entries: Vec<InventoryEntry>,
    ) -> Result<Message, failure::Error> {
        // Check there are some inventory vectors to be added to the message
        if inv_entries.is_empty() {
            Err(BuildersError::NoInvVectorsAnnouncement)?
        }

        // Build the message
        Ok(Message::build_message(
            magic,
            Command::InventoryAnnouncement(InventoryAnnouncement {
                inventory: inv_entries,
            }),
        ))
    }

    /// Function to build GetData messages
    pub fn build_inventory_request(
        magic: u16,
        inv_entries: Vec<InventoryEntry>,
    ) -> Result<Message, failure::Error> {
        // Check there are some inventory vectors to be added to the message
        if inv_entries.is_empty() {
            Err(BuildersError::NoInvVectorsRequest)?
        }

        // Build the message
        Ok(Message::build_message(
            magic,
            Command::InventoryRequest(InventoryRequest {
                inventory: inv_entries,
            }),
        ))
    }

    /// Function to build Block message
    pub fn build_block(
        magic: u16,
        block_header: BlockHeader,
        proof: LeadershipProof,
        txns: BlockTransactions,
    ) -> Message {
        Message::build_message(
            magic,
            Command::Block(Block {
                block_header,
                proof,
                txns,
            }),
        )
    }

    /// Function to build Transaction message
    pub fn build_transaction(magic: u16, transaction: Transaction) -> Message {
        Message::build_message(magic, Command::Transaction(transaction))
    }

    /// Function to build LastBeacon messages
    pub fn build_last_beacon(magic: u16, highest_block_checkpoint: CheckpointBeacon) -> Message {
        Message::build_message(
            magic,
            Command::LastBeacon(LastBeacon {
                highest_block_checkpoint,
            }),
        )
    }

    /// Function to build a message from a command
    fn build_message(magic: u16, command: Command) -> Message {
        Message {
            kind: command,
            magic,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////
// AUX FUNCTIONS
////////////////////////////////////////////////////////////////////////////////////////
/// Function to get a random nonce
fn random_nonce() -> u64 {
    thread_rng().gen()
}

/// Function to build address witnet type from socket addr
fn to_address(socket_addr: SocketAddr) -> Address {
    match socket_addr {
        SocketAddr::V4(addr) => Address {
            ip: {
                let ip = u32::from(addr.ip().to_owned());
                IpAddress::Ipv4 { ip }
            },
            port: addr.port(),
        },
        SocketAddr::V6(addr) => Address {
            ip: {
                let ip = u128::from(addr.ip().to_owned());
                IpAddress::Ipv6 {
                    ip0: ((ip >> 96) & u128::from(U32_MAX)) as u32,
                    ip1: ((ip >> 64) & u128::from(U32_MAX)) as u32,
                    ip2: ((ip >> 32) & u128::from(U32_MAX)) as u32,
                    ip3: (ip & u128::from(U32_MAX)) as u32,
                }
            },
            port: addr.port(),
        },
    }
}

/// Function to build a [SocketAddr](std::net::SocketAddr) from a
/// Witnet [Address](types::Address)
pub fn from_address(addr: &Address) -> SocketAddr {
    let ip: IpAddr = addr.ip.into();
    SocketAddr::from((ip, addr.port))
}

impl From<IpAddress> for IpAddr {
    fn from(addr: IpAddress) -> Self {
        match addr {
            IpAddress::Ipv4 { ip } => IpAddr::V4(Ipv4Addr::from(ip)),
            IpAddress::Ipv6 { ip0, ip1, ip2, ip3 } => {
                let ip = u128::from(ip0) << 96
                    | u128::from(ip1) << 64
                    | u128::from(ip2) << 32
                    | u128::from(ip3);
                IpAddr::V6(Ipv6Addr::from(ip))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_address_ipv4() {
        let socket_addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
        let witnet_addr: Address = to_address(socket_addr);

        assert_eq!(witnet_addr.ip, IpAddress::Ipv4 { ip: 2_130_706_433 });
        assert_eq!(witnet_addr.port, 3000);
    }

    #[test]
    fn test_to_address_ipv6() {
        let socket_addr: SocketAddr = "[::1]:3000".parse().unwrap();
        let witnet_addr: Address = to_address(socket_addr);

        assert_eq!(
            witnet_addr.ip,
            IpAddress::Ipv6 {
                ip0: 0,
                ip1: 0,
                ip2: 0,
                ip3: 1
            }
        );
        assert_eq!(witnet_addr.port, 3000);
    }

    #[test]
    fn test_from_address_ipv4() {
        let witnet_addr: Address = Address {
            ip: IpAddress::Ipv4 { ip: 2_130_706_433 },
            port: 3000,
        };
        let socket_addr: SocketAddr = from_address(&witnet_addr);
        let expected = "127.0.0.1:3000".parse().unwrap();

        assert_eq!(socket_addr, expected);
    }

    #[test]
    fn test_from_address_ipv6() {
        let witnet_addr: Address = Address {
            ip: IpAddress::Ipv6 {
                ip0: 0,
                ip1: 0,
                ip2: 0,
                ip3: 1,
            },
            port: 3000,
        };
        let socket_addr: SocketAddr = from_address(&witnet_addr);
        let expected = "[::1]:3000".parse().unwrap();

        assert_eq!(socket_addr, expected);
    }
}
