extern crate rand;

use std::convert::TryFrom;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use std::u32::MAX as U32_MAX;

use rand::{thread_rng, Rng};

use crate::types::{Address, Command, IpAddress, Message};

////////////////////////////////////////////////////////////////////////////////////////
// PROTOCOL MESSAGES CONSTANTS
////////////////////////////////////////////////////////////////////////////////////////
/// Magic number
pub const MAGIC: u16 = 0xABCD;

/// Version
pub const VERSION: u32 = 0x0000_0001;

/// Capabilities
pub const CAPABILITIES: u64 = 0x0000_0000_0000_0001;

/// User agent
pub const USER_AGENT: &str = "full-node-desktop-edition";

/// Genesis block
pub const GENESIS: u64 = 0x0123_4567_89AB_CDEF;

////////////////////////////////////////////////////////////////////////////////////////
// BUILDER PUBLIC FUNCTIONS
////////////////////////////////////////////////////////////////////////////////////////
/// Function to build GetPeers messages
pub fn build_get_peers() -> Message {
    build_message(Command::GetPeers)
}

/// Function to build Peers messages
pub fn build_peers(peers: &[SocketAddr]) -> Message {
    // Cast all peers to witnet's address struct
    let mut casted_peers = Vec::new();
    peers.iter().for_each(|peer| {
        casted_peers.push(to_address(*peer));
    });

    build_message(Command::Peers {
        peers: casted_peers,
    })
}

/// Function to build Ping messages
pub fn build_ping() -> Message {
    build_message(Command::Ping {
        nonce: random_nonce(),
    })
}

/// Function to build Pong messages
pub fn build_pong(nonce: u64) -> Message {
    build_message(Command::Pong { nonce })
}

/// Function to build Version messages
pub fn build_version(
    sender_addr: SocketAddr,
    receiver_addr: SocketAddr,
    last_epoch: u32,
) -> Message {
    build_message(Command::Version {
        version: VERSION,
        timestamp: current_timestamp(),
        capabilities: CAPABILITIES,
        sender_address: to_address(sender_addr),
        receiver_address: to_address(receiver_addr),
        user_agent: USER_AGENT.to_string(),
        last_epoch,
        genesis: GENESIS,
        nonce: random_nonce(),
    })
}

/// Function to build Verack messages
pub fn build_verack() -> Message {
    build_message(Command::Verack)
}

////////////////////////////////////////////////////////////////////////////////////////
// AUX FUNCTIONS
////////////////////////////////////////////////////////////////////////////////////////
/// Function to build a message from a command
fn build_message(command: Command) -> Message {
    Message {
        kind: command,
        magic: MAGIC,
    }
}

/// Function to get a random nonce
fn random_nonce() -> u64 {
    thread_rng().gen()
}

/// Function to get current timestamp (ms since Unix epoch)
fn current_timestamp() -> u64 {
    let now = SystemTime::now();
    let now_duration = now.duration_since(UNIX_EPOCH).unwrap();
    now_duration.as_secs() * 1000 + u64::from(now_duration.subsec_nanos()) / 1_000_000
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
                    ip0: u32::try_from((ip >> 96) & u128::from(U32_MAX)).unwrap_or(0),
                    ip1: u32::try_from((ip >> 64) & u128::from(U32_MAX)).unwrap_or(0),
                    ip2: u32::try_from((ip >> 32) & u128::from(U32_MAX)).unwrap_or(0),
                    ip3: u32::try_from(ip & u128::from(U32_MAX)).unwrap_or(0),
                }
            },
            port: addr.port(),
        },
    }
}
