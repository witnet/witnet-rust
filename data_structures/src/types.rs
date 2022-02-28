use std::fmt;

use crate::{
    chain::{Block, CheckpointBeacon, Hashable, InventoryEntry, SuperBlock, SuperBlockVote},
    proto::{schema::witnet, ProtobufConvert},
    transaction::Transaction,
};
use serde::{Deserialize, Serialize};

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
// FIXME(#649): Remove clippy skip error
#[allow(clippy::large_enum_variant)]
pub enum Command {
    // Peer discovery messages
    GetPeers(GetPeers),
    Peers(Peers),

    // Handshake messages
    Verack(Verack),
    Version(Version),

    // Inventory messages
    Block(Block),
    Transaction(Transaction),
    SuperBlock(SuperBlock),
    InventoryAnnouncement(InventoryAnnouncement),
    InventoryRequest(InventoryRequest),
    LastBeacon(LastBeacon),

    // Superblock
    SuperBlockVote(SuperBlockVote),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::GetPeers(_) => f.write_str("GET_PEERS"),
            Command::Peers(_) => f.write_str("PEERS"),
            Command::Verack(_) => f.write_str("VERACK"),
            Command::Version(_) => f.write_str("VERSION"),
            Command::Block(block) => write!(
                f,
                "BLOCK: #{}: {}",
                block.block_header.beacon.checkpoint,
                block.hash()
            ),
            Command::InventoryAnnouncement(_) => f.write_str("INVENTORY_ANNOUNCEMENT"),
            Command::InventoryRequest(_) => f.write_str("INVENTORY_REQUEST"),
            Command::LastBeacon(LastBeacon {
                highest_block_checkpoint: h,
                highest_superblock_checkpoint: s,
            }) => write!(
                f,
                "LAST_BEACON: Block: #{}: {} Superblock: #{}: {}",
                h.checkpoint, h.hash_prev_block, s.checkpoint, s.hash_prev_block
            ),
            Command::Transaction(tx) => {
                match tx {
                    Transaction::Commit(_) => f.write_str("COMMIT_TRANSACTION")?,
                    Transaction::ValueTransfer(_) => f.write_str("VALUE_TRANSFER_TRANSACTION")?,
                    Transaction::DataRequest(_) => f.write_str("DATA_REQUEST_TRANSACTION")?,
                    Transaction::Reveal(_) => f.write_str("REVEAL_TRANSACTION")?,
                    Transaction::Tally(_) => f.write_str("TALLY_TRANSACTION")?,
                    Transaction::Mint(_) => f.write_str("MINT_TRANSACTION")?,
                }
                write!(f, ": {}", tx.hash())
            }
            Command::SuperBlockVote(sbv) => write!(
                f,
                "SUPERBLOCK_VOTE {} #{}: {}",
                sbv.secp256k1_signature.public_key.pkh(),
                sbv.superblock_index,
                sbv.superblock_hash
            ),
            Command::SuperBlock(sb) => write!(f, "SUPERBLOCK #{}: {}", sb.index, sb.hash()),
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
    pub nonce: u64,
    pub beacon: LastBeacon,
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

#[derive(Debug, Deserialize, Eq, PartialEq, Clone, ProtobufConvert, Serialize, Hash)]
#[protobuf_convert(pb = "witnet::LastBeacon")]
pub struct LastBeacon {
    pub highest_block_checkpoint: CheckpointBeacon,
    pub highest_superblock_checkpoint: CheckpointBeacon,
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

impl Default for IpAddress {
    fn default() -> Self {
        Self::Ipv4 { ip: 0 }
    }
}

#[derive(Debug, Default, Eq, PartialEq, Clone, Copy)]
pub struct Address {
    pub ip: IpAddress,
    pub port: u16,
}
