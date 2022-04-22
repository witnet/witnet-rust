use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use rand::{thread_rng, Rng};

use witnet_util::timestamp::get_timestamp;

use crate::{
    chain::{
        Block, BlockHeader, BlockTransactions, InventoryEntry, KeyedSignature, SuperBlock,
        SuperBlockVote,
    },
    error::BuildersError,
    transaction::Transaction,
    types::{
        Address, Command, GetPeers, InventoryAnnouncement, InventoryRequest, IpAddress, LastBeacon,
        Message, Peers, Verack, Version,
    },
};

////////////////////////////////////////////////////////////////////////////////////////
// PROTOCOL MESSAGES CONSTANTS
////////////////////////////////////////////////////////////////////////////////////////
/// Protocol version (used in handshake)
pub const PROTOCOL_VERSION: u32 = 0x0000_0001;

/// Capabilities
pub const CAPABILITIES: u64 = 0x0000_0000_0000_0001;

////////////////////////////////////////////////////////////////////////////////////////
// BUILDER PUBLIC FUNCTIONS
////////////////////////////////////////////////////////////////////////////////////////
impl Message {
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
        sender_addr: Option<SocketAddr>,
        receiver_addr: SocketAddr,
        beacon: LastBeacon,
    ) -> Message {
        let addr = sender_addr.map(to_address);
        Message::build_message(
            magic,
            Command::Version(Version {
                version: PROTOCOL_VERSION,
                timestamp: get_timestamp(),
                capabilities: CAPABILITIES,
                sender_address: addr.unwrap_or_default(),
                receiver_address: to_address(receiver_addr),
                user_agent: user_agent(),
                nonce: random_nonce(),
                beacon,
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
            return Err(BuildersError::NoInvVectorsAnnouncement.into());
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
            return Err(BuildersError::NoInvVectorsRequest.into());
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
        block_sig: KeyedSignature,
        txns: BlockTransactions,
    ) -> Message {
        Message::build_message(
            magic,
            Command::Block(Block::new(block_header, block_sig, txns)),
        )
    }

    /// Function to build SuperBlock message
    pub fn build_superblock(magic: u16, superblock: SuperBlock) -> Message {
        Message::build_message(magic, Command::SuperBlock(superblock))
    }

    /// Function to build Transaction message
    pub fn build_transaction(magic: u16, transaction: Transaction) -> Message {
        Message::build_message(magic, Command::Transaction(transaction))
    }

    /// Function to build LastBeacon messages
    pub fn build_last_beacon(magic: u16, last_beacon: LastBeacon) -> Message {
        Message::build_message(magic, Command::LastBeacon(last_beacon))
    }

    /// Function to build SuperBlockVote messages
    pub fn build_superblock_vote(magic: u16, superblock_vote: SuperBlockVote) -> Message {
        Message::build_message(magic, Command::SuperBlockVote(superblock_vote))
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

/// Function that returns the dynamic user agent
pub fn user_agent() -> String {
    // TODO: Read version, arch and OS
    let release = "1.5.2";

    format!("witnet-rust {}", release)
}

fn u128_to_be_u32(x: u128) -> [u32; 4] {
    let ip = x.to_be_bytes();

    [
        u32::from_be_bytes([ip[0], ip[1], ip[2], ip[3]]),
        u32::from_be_bytes([ip[4], ip[5], ip[6], ip[7]]),
        u32::from_be_bytes([ip[8], ip[9], ip[10], ip[11]]),
        u32::from_be_bytes([ip[12], ip[13], ip[14], ip[15]]),
    ]
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
                let [ip0, ip1, ip2, ip3] = u128_to_be_u32(u128::from(addr.ip().to_owned()));
                IpAddress::Ipv6 { ip0, ip1, ip2, ip3 }
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
