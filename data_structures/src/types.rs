use std::fmt;

use crate::chain::{Block, CheckpointBeacon, InventoryEntry, Transaction};

/// Witnet's protocol messages
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Message {
    pub kind: Command,
    pub magic: u16,
}

/// Commands for the Witnet's protocol messages
#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Command {
    // Peer discovery messages
    GetPeers(GetPeers),
    Peers(Peers),

    // Heartbeat messages
    Ping(Ping),
    Pong(Pong),

    // Handshake messages
    Verack(Verack),
    Version(Version),

    // Inventory messages
    Block(Block),
    Transaction(Transaction),
    InventoryAnnouncement(InventoryAnnouncement),
    InventoryRequest(InventoryRequest),
    LastBeacon(LastBeacon),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Command::GetPeers(_) => "GET_PEERS",
                Command::Peers(_) => "PEERS",
                Command::Ping(_) => "PING",
                Command::Pong(_) => "PONG",
                Command::Verack(_) => "VERACK",
                Command::Version(_) => "VERSION",
                Command::Block(_) => "BLOCK",
                Command::InventoryAnnouncement(_) => "INVENTORY_ANNOUNCEMENT",
                Command::InventoryRequest(_) => "INVENTORY_REQUEST",
                Command::LastBeacon(_) => "LAST_BEACON",
                Command::Transaction(_) => "TRANSACTION",
            }
        )
    }
}

///////////////////////////////////////////////////////////
// PEER DISCOVERY MESSAGES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct GetPeers;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Peers {
    pub peers: Vec<Address>,
}

///////////////////////////////////////////////////////////
// HEARTBEAT MESSAGES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Ping {
    pub nonce: u64,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Pong {
    pub nonce: u64,
}

///////////////////////////////////////////////////////////
// HANDSHAKE MESSAGES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Verack;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Version {
    pub version: u32,
    pub timestamp: i64,
    pub capabilities: u64,
    pub sender_address: Address,
    pub receiver_address: Address,
    pub user_agent: String,
    pub last_epoch: u32,
    pub genesis: u64,
    pub nonce: u64,
}

///////////////////////////////////////////////////////////
// INVENTORY MESSAGES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct InventoryAnnouncement {
    pub inventory: Vec<InventoryEntry>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct InventoryRequest {
    pub inventory: Vec<InventoryEntry>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct LastBeacon {
    pub highest_block_checkpoint: CheckpointBeacon,
}

///////////////////////////////////////////////////////////
// AUX TYPES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum IpAddress {
    Ipv4 {
        ip: u32,
    },
    Ipv6 {
        ip0: u32,
        ip1: u32,
        ip2: u32,
        ip3: u32,
    },
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Address {
    pub ip: IpAddress,
    pub port: u16,
}
