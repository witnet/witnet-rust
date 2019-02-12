use std::fmt;

use crate::chain::{Block, CheckpointBeacon, Hashable, InventoryEntry, Transaction};
use crate::proto::{schema::witnet, ProtobufConvert};

/// Witnet's protocol messages
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Message")]
pub struct Message {
    pub kind: Command,
    pub magic: u16,
}

/// Commands for the Witnet's protocol messages
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Message_Command")]
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
        match self {
            Command::GetPeers(_) => f.write_str(&"GET_PEERS".to_string()),
            Command::Peers(_) => f.write_str(&"PEERS".to_string()),
            Command::Ping(_) => f.write_str(&"PING".to_string()),
            Command::Pong(_) => f.write_str(&"PONG".to_string()),
            Command::Verack(_) => f.write_str(&"VERACK".to_string()),
            Command::Version(_) => f.write_str(&"VERSION".to_string()),
            Command::Block(block) => f.write_str(&format!("BLOCK: {}", block.hash())),
            Command::InventoryAnnouncement(_) => f.write_str(&"INVENTORY_ANNOUNCEMENT".to_string()),
            Command::InventoryRequest(_) => f.write_str(&"INVENTORY_REQUEST".to_string()),
            Command::LastBeacon(_) => f.write_str(&"LAST_BEACON".to_string()),
            Command::Transaction(_) => f.write_str(&"TRANSACTION".to_string()),
        }
    }
}

///////////////////////////////////////////////////////////
// PEER DISCOVERY MESSAGES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::GetPeers")]
pub struct GetPeers;

#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Peers")]
pub struct Peers {
    pub peers: Vec<Address>,
}

///////////////////////////////////////////////////////////
// HEARTBEAT MESSAGES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Ping")]
pub struct Ping {
    pub nonce: u64,
}

#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Pong")]
pub struct Pong {
    pub nonce: u64,
}

///////////////////////////////////////////////////////////
// HANDSHAKE MESSAGES
///////////////////////////////////////////////////////////
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Verack")]
pub struct Verack;

#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::Version")]
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
#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::InventoryAnnouncement")]
pub struct InventoryAnnouncement {
    pub inventory: Vec<InventoryEntry>,
}

#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::InventoryRequest")]
pub struct InventoryRequest {
    pub inventory: Vec<InventoryEntry>,
}

#[derive(Debug, Eq, PartialEq, Clone, ProtobufConvert)]
#[protobuf_convert(pb = "witnet::LastBeacon")]
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
