extern crate flatbuffers;

use std::convert::Into;

use crate::chain::{
    Block, BlockHeader, CheckpointBeacon, Epoch, Hash, Input, InventoryEntry, KeyedSignature,
    LeadershipProof, Output, Signature, Transaction,
};
use crate::flatbuffers::protocol_generated::protocol;

use crate::types::{
    Address, Command, GetPeers, InventoryAnnouncement, InventoryRequest, IpAddress,
    IpAddress::{Ipv4, Ipv6},
    LastBeacon, Message, Peers, Ping, Pong, Verack, Version,
};

use flatbuffers::FlatBufferBuilder;

pub const FTB_SIZE: usize = 1024;

type WIPOffsetAddress<'a> = flatbuffers::WIPOffset<protocol::Address<'a>>;
type WIPOffsetAddresses<'a> = flatbuffers::WIPOffset<
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::Address<'a>>>,
>;
type WIPOffsetBlock<'a> = flatbuffers::WIPOffset<protocol::Block<'a>>;
type WIPOffsetBlockHeader<'a> = flatbuffers::WIPOffset<protocol::BlockHeader<'a>>;
type WIPOffsetBlockMessage<'a> = flatbuffers::WIPOffset<protocol::Block<'a>>;
type WIPOffsetCheckpointBeacon<'a> = flatbuffers::WIPOffset<protocol::CheckpointBeacon<'a>>;
type WIPOffsetGetPeers<'a> = flatbuffers::WIPOffset<protocol::GetPeers<'a>>;
type WIPOffsetHash<'a> = flatbuffers::WIPOffset<protocol::Hash<'a>>;
type WIPOffsetInputVector<'a> = flatbuffers::WIPOffset<
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::Input<'a>>>,
>;
type WIPOffsetInventoryAnnouncement<'a> =
    flatbuffers::WIPOffset<protocol::InventoryAnnouncement<'a>>;
type WIPOffsetInventoryEntry<'a> = flatbuffers::WIPOffset<protocol::InventoryEntry<'a>>;
type WIPOffsetInventoryEntryVector<'a> = flatbuffers::WIPOffset<
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::InventoryEntry<'a>>>,
>;
type WIPOffsetInventoryRequest<'a> = flatbuffers::WIPOffset<protocol::InventoryRequest<'a>>;
type WIPOffsetIpV4<'a> = flatbuffers::WIPOffset<protocol::Ipv4<'a>>;
type WIPOffsetIpV6<'a> = flatbuffers::WIPOffset<protocol::Ipv6<'a>>;
type WIPOffsetLastBeacon<'a> = flatbuffers::WIPOffset<protocol::LastBeacon<'a>>;
type WIPOffsetLeadershipProof<'a> = flatbuffers::WIPOffset<protocol::LeadershipProof<'a>>;
type WIPOffsetMessage<'a> = flatbuffers::WIPOffset<protocol::Message<'a>>;
type WIPOffsetOutputVector<'a> = flatbuffers::WIPOffset<
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::Output<'a>>>,
>;
type WIPOffsetPeersMessage<'a> = flatbuffers::WIPOffset<protocol::Peers<'a>>;
type WIPOffsetPing<'a> = flatbuffers::WIPOffset<protocol::Ping<'a>>;
type WIPOffsetPong<'a> = flatbuffers::WIPOffset<protocol::Pong<'a>>;
type WIPOffsetTransaction<'a> = flatbuffers::WIPOffset<protocol::Transaction<'a>>;
type WIPOffsetTransactionVector<'a> = Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::Transaction<'a>>>,
    >,
>;
type WIPOffsetUnion = Option<flatbuffers::WIPOffset<flatbuffers::UnionWIPOffset>>;
type WIPOffsetVerackMessage<'a> = flatbuffers::WIPOffset<protocol::Verack<'a>>;
type WIPOffsetVersionMessage<'a> = flatbuffers::WIPOffset<protocol::Version<'a>>;
type WOIPOffsetKeyedSignatureVector<'a> = flatbuffers::WIPOffset<
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::KeyedSignature<'a>>>,
>;

////////////////////////////////////////////////////////
// ARGS
////////////////////////////////////////////////////////
#[derive(Debug, Clone)]
pub struct BlockArgs {
    pub block_header: BlockHeader,
    pub proof: LeadershipProof,
    pub txns: Vec<Transaction>,
}

pub struct TransactionArgs {
    pub version: u32,
    pub inputs: Vec<Input>,
    pub outputs: Vec<Output>,
    pub signatures: Vec<KeyedSignature>,
}

// COMMAND ARGS
#[derive(Debug, Clone, Copy)]
struct EmptyCommandArgs {
    magic: u16,
}

// Peer discovery
#[derive(Debug, Clone, Copy)]
struct PeersFlatbufferArgs<'a> {
    magic: u16,
    peers: &'a [Address],
}

#[derive(Debug, Clone, Copy)]
struct PeersWitnetArgs<'a> {
    magic: u16,
    peers: protocol::Peers<'a>,
}

// Heartbeat
#[derive(Debug, Clone, Copy)]
struct HeartbeatCommandsArgs {
    magic: u16,
    nonce: u64,
}

// Handshake
#[derive(Debug, Clone, Copy)]
struct VersionCommandArgs<'a> {
    magic: u16,
    capabilities: u64,
    genesis: u64,
    last_epoch: u32,
    nonce: u64,
    receiver_address: &'a Address,
    sender_address: &'a Address,
    timestamp: i64,
    user_agent: &'a str,
    version: u32,
}

// Inventory
#[derive(Debug, Clone, Copy)]
struct LastBeaconCommandArgs {
    magic: u16,
    highest_block_checkpoint: CheckpointBeacon,
}

#[derive(Debug, Clone)]
pub struct BlockCommandArgs<'a> {
    pub magic: u16,
    pub block_header: BlockHeader,
    pub proof: LeadershipProof,
    pub txns: &'a [Transaction],
}

#[derive(Debug, Clone, Copy)]
struct InventoryAnnouncementWitnetArgs<'a> {
    magic: u16,
    inventory: protocol::InventoryAnnouncement<'a>,
}

#[derive(Debug, Clone, Copy)]
struct InventoryRequestWitnetArgs<'a> {
    magic: u16,
    inventory: protocol::InventoryRequest<'a>,
}

#[derive(Debug, Clone, Copy)]
struct InventoryArgs<'a> {
    magic: u16,
    inventory: &'a [InventoryEntry],
}

pub struct TransactionsVectorArgs<'a> {
    txns: &'a [Transaction],
}

pub struct PeersArgs<'a> {
    peers: &'a [Address],
}

pub struct VersionMessageArgs {
    version: u32,
    timestamp: i64,
    capabilities: u64,
    sender_ip: IpAddress,
    sender_port: u16,
    receiver_ip: IpAddress,
    receiver_port: u16,
    user_agent: String,
    last_epoch: u32,
    genesis: u64,
    nonce: u64,
}

pub struct BlockMessageArgs<'a> {
    version: u32,
    checkpoint: u32,
    hash: Hash,
    hash_prev_block: Hash,
    block_sig: Option<Signature>,
    influence: u64,
    txns: &'a [Transaction],
}

pub struct InventoryWipoffsetArgs<'a> {
    inventory_entries: &'a [InventoryEntry],
}

pub struct InventoryEntryArgs<'a> {
    inv_item: &'a InventoryEntry,
}

pub struct LastBeaconWipoffsetArgs {
    checkpoint: Epoch,
    hash_prev_block: Hash,
}

pub struct BlockWipoffsetArgs<'a> {
    txns: &'a [Transaction],
    version: u32,
    hash: Hash,
    checkpoint: u32,
    hash_prev_block: Hash,
    block_sig: Option<Signature>,
    influence: u64,
}

pub struct CheckpointBeaconArgs {
    pub checkpoint: u32,
    pub hash_prev_block: Hash,
}

pub struct BlockHeaderArgs {
    version: u32,
    hash: Hash,
    checkpoint: u32,
    hash_prev_block: Hash,
}

pub struct HashArgs {
    hash: Hash,
}

pub struct HeartbeatArgs {
    nonce: u64,
}

pub struct LeadershipProofArgs {
    block_sig: Option<Signature>,
    influence: u64,
}

pub struct MessageArgs {
    magic: u16,
    command_type: protocol::Command,
    command: WIPOffsetUnion,
}

pub struct IpV4Args {
    ip: u32,
}

pub struct IpV6Args {
    ip0: u32,
    ip1: u32,
    ip2: u32,
    ip3: u32,
}

pub struct AddressArgs {
    ip: IpAddress,
    port: u16,
}

////////////////////////////////////////////////////////
// INTO TRAIT (Message ----> Vec<u8>)
////////////////////////////////////////////////////////
impl Into<Vec<u8>> for Message {
    fn into(self) -> Vec<u8> {
        // Build builder to create flatbuffers to encode Witnet messages
        let mut builder = flatbuffers::FlatBufferBuilder::new_with_capacity(FTB_SIZE);

        // Build flatbuffer to encode a Witnet message
        match self.kind {
            // Heartbeat
            Command::Ping(Ping { nonce }) => build_ping_message_flatbuffer(
                &mut builder,
                HeartbeatCommandsArgs {
                    magic: self.magic,
                    nonce,
                },
            ),

            Command::Pong(Pong { nonce }) => build_pong_message_flatbuffer(
                &mut builder,
                HeartbeatCommandsArgs {
                    magic: self.magic,
                    nonce,
                },
            ),

            // Peer discovery
            Command::GetPeers(GetPeers) => build_get_peers_message_flatbuffer(
                &mut builder,
                EmptyCommandArgs { magic: self.magic },
            ),

            Command::Peers(Peers { peers }) => build_peers_message_flatbuffer(
                &mut builder,
                PeersFlatbufferArgs {
                    magic: self.magic,
                    peers: &peers,
                },
            ),

            // Handshake
            Command::Verack(Verack) => build_verack_message_flatbuffer(
                &mut builder,
                EmptyCommandArgs { magic: self.magic },
            ),

            Command::Version(Version {
                version,
                timestamp,
                capabilities,
                sender_address,
                receiver_address,
                user_agent,
                last_epoch,
                genesis,
                nonce,
            }) => build_version_message_flatbuffer(
                &mut builder,
                VersionCommandArgs {
                    magic: self.magic,
                    version,
                    timestamp,
                    capabilities,
                    sender_address: &sender_address,
                    receiver_address: &receiver_address,
                    user_agent: &user_agent,
                    last_epoch,
                    genesis,
                    nonce,
                },
            ),

            Command::Block(Block {
                block_header,
                proof,
                txns,
            }) => build_block_message_flatbuffer(
                &mut builder,
                &BlockCommandArgs {
                    magic: self.magic,
                    block_header,
                    proof,
                    txns: &txns,
                },
            ),

            Command::InventoryAnnouncement(InventoryAnnouncement { inventory }) => {
                build_inv_announcement_message_flatbuffer(
                    &mut builder,
                    InventoryArgs {
                        magic: self.magic,
                        inventory: &inventory,
                    },
                )
            }

            Command::InventoryRequest(InventoryRequest { inventory }) => {
                build_inv_request_message_flatbuffer(
                    &mut builder,
                    InventoryArgs {
                        magic: self.magic,
                        inventory: &inventory,
                    },
                )
            }

            Command::LastBeacon(LastBeacon {
                highest_block_checkpoint,
            }) => build_last_beacon_message_flatbuffer(
                &mut builder,
                LastBeaconCommandArgs {
                    magic: self.magic,
                    highest_block_checkpoint,
                },
            ),
        }
    }
}

/////////////////////
// FBT BUILDERS
/////////////////////

// Build a Block flatbuffer (used for block id based on digest)
pub fn build_block_flatbuffer(
    builder: Option<&mut FlatBufferBuilder>,
    block_args: &BlockArgs,
) -> Vec<u8> {
    let aux_builder: &mut FlatBufferBuilder =
        &mut flatbuffers::FlatBufferBuilder::new_with_capacity(FTB_SIZE);
    let builder = builder.unwrap_or_else(|| aux_builder);
    let block_wipoffset = build_block_wipoffset(
        builder,
        &BlockWipoffsetArgs {
            txns: &block_args.txns,
            version: block_args.block_header.version,
            checkpoint: block_args.block_header.beacon.checkpoint,
            hash_prev_block: block_args.block_header.beacon.hash_prev_block,
            hash: block_args.block_header.hash_merkle_root,
            block_sig: block_args.proof.block_sig,
            influence: block_args.proof.influence,
        },
    );
    // Build block flatbuffer
    builder.finish(block_wipoffset, None);
    builder.finished_data().to_vec()
}

// Build a Transaction flatbuffer (used for transaction id based on digest)
pub fn build_transaction_flatbuffer(
    builder: Option<&mut FlatBufferBuilder>,
    transaction_args: &TransactionArgs,
) -> Vec<u8> {
    let aux_builder: &mut FlatBufferBuilder =
        &mut flatbuffers::FlatBufferBuilder::new_with_capacity(FTB_SIZE);
    let builder = builder.unwrap_or_else(|| aux_builder);

    let input_vector_wipoffset = build_input_vector_wipoffset(builder, &transaction_args.inputs);
    let output_vector_wipoffset = build_output_vector_wipoffset(builder, &transaction_args.outputs);
    let signatures_vector_wipoffset =
        build_keyed_signature_vector_wipoffset(builder, &transaction_args.signatures);

    let transaction_wipoffset = protocol::Transaction::create(
        builder,
        &protocol::TransactionArgs {
            version: transaction_args.version,
            inputs: Some(input_vector_wipoffset),
            outputs: Some(output_vector_wipoffset),
            signatures: Some(signatures_vector_wipoffset),
        },
    );

    // Build transaction flatbuffer
    builder.finish(transaction_wipoffset, None);
    builder.finished_data().to_vec()
}

// Build a Block flatbuffer to encode a Witnet's Block message
fn build_block_message_flatbuffer<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    block_args: &BlockCommandArgs<'a>,
) -> Vec<u8> {
    let block_command_wipoffset = build_block_command_wipoffset(
        builder,
        &BlockMessageArgs {
            version: block_args.block_header.version,
            checkpoint: block_args.block_header.beacon.checkpoint,
            hash_prev_block: block_args.block_header.beacon.hash_prev_block,
            hash: block_args.block_header.hash_merkle_root,
            block_sig: block_args.proof.block_sig,
            influence: block_args.proof.influence,
            txns: block_args.txns,
        },
    );
    let block_message_wipoffset = build_message_wipoffset(
        builder,
        &MessageArgs {
            magic: block_args.magic,
            command_type: protocol::Command::Block,
            command: Some(block_command_wipoffset.as_union_value()),
        },
    );

    build_message_flatbuffer(builder, block_message_wipoffset)
}

/// Build CheckpointBeacon flatbuffer
pub fn build_checkpoint_beacon_flatbuffer(
    builder: Option<&mut FlatBufferBuilder>,
    checkpoint_beacon_args: &CheckpointBeaconArgs,
) -> Vec<u8> {
    let aux_builder: &mut FlatBufferBuilder =
        &mut flatbuffers::FlatBufferBuilder::new_with_capacity(FTB_SIZE);
    let builder = builder.unwrap_or_else(|| aux_builder);

    let checkpoint_beacon_wipoffset =
        build_checkpoint_beacon_wipoffset(builder, checkpoint_beacon_args);
    builder.finish(checkpoint_beacon_wipoffset, None);
    builder.finished_data().to_vec()
}

// Build a GetPeers flatbuffer to encode Witnet's GetPeers message
fn build_get_peers_message_flatbuffer(
    builder: &mut FlatBufferBuilder,
    get_peers_args: EmptyCommandArgs,
) -> Vec<u8> {
    let get_peers_command_wipoffset = build_get_peers_command_wipoffset(builder);
    let get_peers_message_wipoffset = build_message_wipoffset(
        builder,
        &MessageArgs {
            magic: get_peers_args.magic,
            command_type: protocol::Command::GetPeers,
            command: Some(get_peers_command_wipoffset.as_union_value()),
        },
    );

    build_message_flatbuffer(builder, get_peers_message_wipoffset)
}

// Build an InventoryAnnouncement flatbuffer to encode a Witnet's InventoryAnnouncement message
fn build_inv_announcement_message_flatbuffer<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    inv_args: InventoryArgs<'a>,
) -> Vec<u8> {
    let inv_announcement_command_wipoffset = build_inv_announcement_wipoffset(
        builder,
        &InventoryWipoffsetArgs {
            inventory_entries: inv_args.inventory,
        },
    );
    let message_wipoffset = build_message_wipoffset(
        builder,
        &MessageArgs {
            magic: inv_args.magic,
            command_type: protocol::Command::InventoryAnnouncement,
            command: Some(inv_announcement_command_wipoffset.as_union_value()),
        },
    );

    // Get vector of bytes from flatbuffer message
    build_message_flatbuffer(builder, message_wipoffset)
}

// Build an InventoryRequest flatbuffer to encode a Witnet's InventoryRequest message
fn build_inv_request_message_flatbuffer<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    get_data_args: InventoryArgs<'a>,
) -> Vec<u8> {
    let inventory_request_command_wipoffset = build_inv_request_command_wipoffset(
        builder,
        &InventoryWipoffsetArgs {
            inventory_entries: get_data_args.inventory,
        },
    );
    let message_wipoffset = build_message_wipoffset(
        builder,
        &MessageArgs {
            magic: get_data_args.magic,
            command_type: protocol::Command::InventoryRequest,
            command: Some(inventory_request_command_wipoffset.as_union_value()),
        },
    );

    // Get vector of bytes from flatbuffer message
    build_message_flatbuffer(builder, message_wipoffset)
}

// Build a LastBeacon flatbuffer to encode a Witnet LastBeacon message
fn build_last_beacon_message_flatbuffer(
    builder: &mut FlatBufferBuilder,
    last_beacon_args: LastBeaconCommandArgs,
) -> Vec<u8> {
    let last_beacon_command_wipoffset = build_last_beacon_command_wipoffset(
        builder,
        &LastBeaconWipoffsetArgs {
            checkpoint: last_beacon_args.highest_block_checkpoint.checkpoint,
            hash_prev_block: last_beacon_args.highest_block_checkpoint.hash_prev_block,
        },
    );
    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: last_beacon_args.magic,
            command_type: protocol::Command::LastBeacon,
            command: Some(last_beacon_command_wipoffset.as_union_value()),
        },
    );

    build_message_flatbuffer(builder, message)
}

// Convert a flatbuffers message into a vector of bytes
fn build_message_flatbuffer(
    builder: &mut FlatBufferBuilder,
    message: flatbuffers::WIPOffset<protocol::Message>,
) -> Vec<u8> {
    builder.finish(message, None);
    builder.finished_data().to_vec()
}

// Build a Ping flatbuffer to encode a Witnet's Ping message
fn build_ping_message_flatbuffer(
    builder: &mut FlatBufferBuilder,
    ping_args: HeartbeatCommandsArgs,
) -> Vec<u8> {
    let ping_command_wipoffset = build_ping_command_wipoffset(
        builder,
        &HeartbeatArgs {
            nonce: ping_args.nonce,
        },
    );

    let ping_message_wipoffset = build_message_wipoffset(
        builder,
        &MessageArgs {
            magic: ping_args.magic,
            command_type: protocol::Command::Ping,
            command: Some(ping_command_wipoffset.as_union_value()),
        },
    );

    build_message_flatbuffer(builder, ping_message_wipoffset)
}

// Build a Peers flatbuffer to encode a Witnet's Peers message
fn build_peers_message_flatbuffer<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    peers_args: PeersFlatbufferArgs<'a>,
) -> Vec<u8> {
    let peers_command_wipoffset = build_peers_message_wipoffset(
        builder,
        &PeersArgs {
            peers: peers_args.peers,
        },
    );
    let peers_message_wipoffset = build_message_wipoffset(
        builder,
        &MessageArgs {
            magic: peers_args.magic,
            command_type: protocol::Command::Peers,
            command: Some(peers_command_wipoffset.as_union_value()),
        },
    );

    build_message_flatbuffer(builder, peers_message_wipoffset)
}

// Build a Pong flatbuffer to encode a Witnet's Pong message
fn build_pong_message_flatbuffer(
    builder: &mut FlatBufferBuilder,
    pong_args: HeartbeatCommandsArgs,
) -> Vec<u8> {
    let pong_command_wipoffset = build_pong_command_wipoffset(
        builder,
        &HeartbeatArgs {
            nonce: pong_args.nonce,
        },
    );
    let pong_message_wipoffset = build_message_wipoffset(
        builder,
        &MessageArgs {
            magic: pong_args.magic,
            command_type: protocol::Command::Pong,
            command: Some(pong_command_wipoffset.as_union_value()),
        },
    );

    build_message_flatbuffer(builder, pong_message_wipoffset)
}

// Build a Verack flatbuffer to encode a Witnet's Verack message
fn build_verack_message_flatbuffer(
    builder: &mut FlatBufferBuilder,
    verack_args: EmptyCommandArgs,
) -> Vec<u8> {
    let verack_command_wipoffset = build_verack_message_wipoffset(builder);
    let verack_message_wipoffset = build_message_wipoffset(
        builder,
        &MessageArgs {
            magic: verack_args.magic,
            command_type: protocol::Command::Verack,
            command: Some(verack_command_wipoffset.as_union_value()),
        },
    );

    build_message_flatbuffer(builder, verack_message_wipoffset)
}

// Build a Version flatbuffer to encode a Witnet's Version message
fn build_version_message_flatbuffer(
    builder: &mut FlatBufferBuilder,
    version_args: VersionCommandArgs,
) -> Vec<u8> {
    let version_command_wipoffset = build_version_message_wipoffset(
        builder,
        &VersionMessageArgs {
            version: version_args.version,
            timestamp: version_args.timestamp,
            capabilities: version_args.capabilities,
            sender_ip: version_args.sender_address.ip,
            sender_port: version_args.sender_address.port,
            receiver_ip: version_args.receiver_address.ip,
            receiver_port: version_args.receiver_address.port,
            user_agent: version_args.user_agent.to_string(),
            last_epoch: version_args.last_epoch,
            genesis: version_args.genesis,
            nonce: version_args.nonce,
        },
    );
    let version_message_wipoffset = build_message_wipoffset(
        builder,
        &MessageArgs {
            magic: version_args.magic,
            command_type: protocol::Command::Version,
            command: Some(version_command_wipoffset.as_union_value()),
        },
    );

    build_message_flatbuffer(builder, version_message_wipoffset)
}

/////////////////////////////
// WIPOFFSET BUILDERS
/////////////////////////////
pub fn build_address_vector_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    addresses_args: &PeersArgs<'a>,
) -> WIPOffsetAddresses<'a> {
    let ftb_addresses: Vec<flatbuffers::WIPOffset<protocol::Address>> = addresses_args
        .peers
        .iter()
        .map(|peer: &Address| {
            build_address_wipoffset(
                builder,
                &AddressArgs {
                    ip: peer.ip,
                    port: peer.port,
                },
            )
        })
        .collect();
    builder.create_vector(&ftb_addresses)
}

pub fn build_address_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    address_args: &AddressArgs,
) -> WIPOffsetAddress<'a> {
    match address_args.ip {
        Ipv4 { ip } => {
            let ip_v4_wipoffset = build_ipv4_wipoffset(builder, &IpV4Args { ip });
            protocol::Address::create(
                builder,
                &protocol::AddressArgs {
                    ip_type: protocol::IpAddress::Ipv4,
                    ip: Some(ip_v4_wipoffset.as_union_value()),
                    port: address_args.port,
                },
            )
        }
        Ipv6 { ip0, ip1, ip2, ip3 } => {
            let ipv6_wipoffset = build_ipv6_wipoffset(builder, &IpV6Args { ip0, ip1, ip2, ip3 });
            protocol::Address::create(
                builder,
                &protocol::AddressArgs {
                    ip_type: protocol::IpAddress::Ipv6,
                    ip: Some(ipv6_wipoffset.as_union_value()),
                    port: address_args.port,
                },
            )
        }
    }
}

pub fn build_block_command_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    block_message_args: &BlockMessageArgs<'a>,
) -> WIPOffsetBlockMessage<'a> {
    let txns_vector_wipoffset = build_transactions_vector_wipoffset(
        builder,
        &TransactionsVectorArgs {
            txns: block_message_args.txns,
        },
    );
    let block_header_wipoffset = Some(build_block_header_wipoffset(
        builder,
        &BlockHeaderArgs {
            version: block_message_args.version,
            checkpoint: block_message_args.checkpoint,
            hash_prev_block: block_message_args.hash_prev_block,
            hash: block_message_args.hash,
        },
    ));

    let proof_wipoffset = build_leadership_proof_wipoffset(
        builder,
        &LeadershipProofArgs {
            block_sig: block_message_args.block_sig,
            influence: block_message_args.influence,
        },
    );
    protocol::Block::create(
        builder,
        &protocol::BlockArgs {
            block_header: block_header_wipoffset,
            proof: Some(proof_wipoffset),
            txns: txns_vector_wipoffset,
        },
    )
}

pub fn build_block_header_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    block_headers_args: &BlockHeaderArgs,
) -> WIPOffsetBlockHeader<'a> {
    let hash_merkle_root = Some(build_hash_wipoffset(
        builder,
        &HashArgs {
            hash: block_headers_args.hash,
        },
    ));
    let checkpoint_beacon_wipoffset = Some(build_checkpoint_beacon_wipoffset(
        builder,
        &CheckpointBeaconArgs {
            checkpoint: block_headers_args.checkpoint,
            hash_prev_block: block_headers_args.hash_prev_block,
        },
    ));
    protocol::BlockHeader::create(
        builder,
        &protocol::BlockHeaderArgs {
            version: block_headers_args.version,
            beacon: checkpoint_beacon_wipoffset,
            hash_merkle_root,
        },
    )
}

pub fn build_block_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    block_args: &BlockWipoffsetArgs,
) -> WIPOffsetBlock<'a> {
    let proof_wipoffset = Some(build_leadership_proof_wipoffset(
        builder,
        &LeadershipProofArgs {
            block_sig: block_args.block_sig,
            influence: block_args.influence,
        },
    ));
    // Build block header flatbuffer
    let block_header_wipoffset = Some(build_block_header_wipoffset(
        builder,
        &BlockHeaderArgs {
            version: block_args.version,
            checkpoint: block_args.checkpoint,
            hash_prev_block: block_args.hash_prev_block,
            hash: block_args.hash,
        },
    ));
    // Build transaction array flatbuffer
    let txns_vector_wipoffset = build_transactions_vector_wipoffset(
        builder,
        &TransactionsVectorArgs {
            txns: block_args.txns,
        },
    );
    // Build block command flatbuffer
    protocol::Block::create(
        builder,
        &protocol::BlockArgs {
            block_header: block_header_wipoffset,
            proof: proof_wipoffset,
            txns: txns_vector_wipoffset,
        },
    )
}

pub fn build_checkpoint_beacon_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    beacon_args: &CheckpointBeaconArgs,
) -> WIPOffsetCheckpointBeacon<'a> {
    let hash_prev_block_wipoffset = Some(build_hash_wipoffset(
        builder,
        &HashArgs {
            hash: beacon_args.hash_prev_block,
        },
    ));
    protocol::CheckpointBeacon::create(
        builder,
        &protocol::CheckpointBeaconArgs {
            checkpoint: beacon_args.checkpoint,
            hash_prev_block: hash_prev_block_wipoffset,
        },
    )
}

pub fn build_get_peers_command_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
) -> WIPOffsetGetPeers<'a> {
    protocol::GetPeers::create(builder, &protocol::GetPeersArgs {})
}

pub fn build_inv_announcement_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    inventory_announcement_args: &InventoryWipoffsetArgs<'a>,
) -> WIPOffsetInventoryAnnouncement<'a> {
    let inventory_entry_vector_wipoffset = build_inv_entry_vector_wipoffset(
        builder,
        &InventoryWipoffsetArgs {
            inventory_entries: inventory_announcement_args.inventory_entries,
        },
    );
    // Build inv flatbuffers command
    protocol::InventoryAnnouncement::create(
        builder,
        &protocol::InventoryAnnouncementArgs {
            inventory: Some(inventory_entry_vector_wipoffset),
        },
    )
}

pub fn build_inv_entry_vector_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    inventory_entry_vector_args: &InventoryWipoffsetArgs<'a>,
) -> WIPOffsetInventoryEntryVector<'a> {
    // Build vector of flatbuffers' inv items
    let inventory_entry_wipoffset: Vec<flatbuffers::WIPOffset<protocol::InventoryEntry>> =
        inventory_entry_vector_args
            .inventory_entries
            .iter()
            .map(|inv_item: &InventoryEntry| {
                build_inv_entry_wipoffset(builder, &InventoryEntryArgs { inv_item })
            })
            .collect();

    // Build flatbuffers' vector of flatbuffers' inv items
    builder.create_vector(&inventory_entry_wipoffset)
}

pub fn build_inv_entry_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    inventory_entry_args: &InventoryEntryArgs,
) -> WIPOffsetInventoryEntry<'a> {
    // Build flatbuffers' hash bytes
    let hash = match inventory_entry_args.inv_item {
        InventoryEntry::Error(hash)
        | InventoryEntry::Tx(hash)
        | InventoryEntry::Block(hash)
        | InventoryEntry::DataRequest(hash)
        | InventoryEntry::DataResult(hash) => hash,
    };

    let ftb_hash = build_hash_wipoffset(builder, &HashArgs { hash: *hash });
    // Build flatbuffers inv vector type
    let ftb_type = match inventory_entry_args.inv_item {
        InventoryEntry::Error(_) => protocol::InventoryItemType::Error,
        InventoryEntry::Tx(_) => protocol::InventoryItemType::Tx,
        InventoryEntry::Block(_) => protocol::InventoryItemType::Block,
        InventoryEntry::DataRequest(_) => protocol::InventoryItemType::DataRequest,
        InventoryEntry::DataResult(_) => protocol::InventoryItemType::DataResult,
    };

    // Build flatbuffers inv vector
    protocol::InventoryEntry::create(
        builder,
        &protocol::InventoryEntryArgs {
            type_: ftb_type,
            hash: Some(ftb_hash),
        },
    )
}

pub fn build_inv_request_command_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    inventory_request_args: &InventoryWipoffsetArgs<'a>,
) -> WIPOffsetInventoryRequest<'a> {
    let inventory_entry_vector_wipoffset = build_inv_entry_vector_wipoffset(
        builder,
        &InventoryWipoffsetArgs {
            inventory_entries: inventory_request_args.inventory_entries,
        },
    );
    // Build get_data flatbuffers command
    protocol::InventoryRequest::create(
        builder,
        &protocol::InventoryRequestArgs {
            inventory: Some(inventory_entry_vector_wipoffset),
        },
    )
}

pub fn build_ipv4_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    ip_v4_args: &IpV4Args,
) -> WIPOffsetIpV4<'a> {
    protocol::Ipv4::create(builder, &protocol::Ipv4Args { ip: ip_v4_args.ip })
}

pub fn build_ipv6_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    ip_v6_args: &IpV6Args,
) -> WIPOffsetIpV6<'a> {
    protocol::Ipv6::create(
        builder,
        &protocol::Ipv6Args {
            ip0: ip_v6_args.ip0,
            ip1: ip_v6_args.ip1,
            ip2: ip_v6_args.ip2,
            ip3: ip_v6_args.ip3,
        },
    )
}

pub fn build_hash_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    hash_args: &HashArgs,
) -> WIPOffsetHash<'a> {
    let hash = match hash_args.hash {
        Hash::SHA256(sha256) => protocol::HashArgs {
            type_: protocol::HashType::SHA256,
            bytes: Some(builder.create_vector(&sha256)),
        },
    };
    protocol::Hash::create(builder, &hash)
}

pub fn build_last_beacon_command_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    last_beacon_args: &LastBeaconWipoffsetArgs,
) -> WIPOffsetLastBeacon<'a> {
    let beacon = build_checkpoint_beacon_wipoffset(
        builder,
        &CheckpointBeaconArgs {
            checkpoint: last_beacon_args.checkpoint,
            hash_prev_block: last_beacon_args.hash_prev_block,
        },
    );

    protocol::LastBeacon::create(
        builder,
        &protocol::LastBeaconArgs {
            highest_block_checkpoint: Some(beacon),
        },
    )
}

pub fn build_leadership_proof_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    leadership_proof_args: &LeadershipProofArgs,
) -> WIPOffsetLeadershipProof<'a> {
    let block_sig_type = leadership_proof_args
        .block_sig
        .map(|signature| match signature {
            Signature::Secp256k1(_) => protocol::Signature::Secp256k1Signature,
        });
    let block_sig = leadership_proof_args
        .block_sig
        .map(|signature| match signature {
            Signature::Secp256k1(secp256k1) => {
                let mut s = secp256k1.s.to_vec();
                s.push(secp256k1.v);
                let r_ftb = Some(builder.create_vector(&secp256k1.r));
                let s_ftb = Some(builder.create_vector(&s));

                protocol::Secp256k1Signature::create(
                    builder,
                    &protocol::Secp256k1SignatureArgs { r: r_ftb, s: s_ftb },
                )
                .as_union_value()
            }
        });

    protocol::LeadershipProof::create(
        builder,
        &protocol::LeadershipProofArgs {
            block_sig_type: block_sig_type.unwrap_or(protocol::Signature::NONE),
            block_sig,
            influence: leadership_proof_args.influence,
        },
    )
}

pub fn build_message_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    message_args: &MessageArgs,
) -> WIPOffsetMessage<'a> {
    protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: message_args.magic,
            command_type: message_args.command_type,
            command: message_args.command,
        },
    )
}

pub fn build_peers_message_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    peers_message_args: &PeersArgs<'a>,
) -> WIPOffsetPeersMessage<'a> {
    let addresses_wipoffset = build_address_vector_wipoffset(
        builder,
        &PeersArgs {
            peers: peers_message_args.peers,
        },
    );
    protocol::Peers::create(
        builder,
        &protocol::PeersArgs {
            peers: Some(addresses_wipoffset),
        },
    )
}

pub fn build_ping_command_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    ping_args: &HeartbeatArgs,
) -> WIPOffsetPing<'a> {
    protocol::Ping::create(
        builder,
        &protocol::PingArgs {
            nonce: ping_args.nonce,
        },
    )
}

pub fn build_pong_command_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    pong_args: &HeartbeatArgs,
) -> WIPOffsetPong<'a> {
    protocol::Pong::create(
        builder,
        &protocol::PongArgs {
            nonce: pong_args.nonce,
        },
    )
}

pub fn build_transactions_vector_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    transactions_vector_args: &TransactionsVectorArgs,
) -> WIPOffsetTransactionVector<'a> {
    let txns: Vec<WIPOffsetTransaction> = transactions_vector_args
        .txns
        .iter()
        .map(|tx: &Transaction| {
            let input_vector_wipoffset = build_input_vector_wipoffset(builder, &tx.inputs);
            let output_vector_wipoffset = build_output_vector_wipoffset(builder, &tx.outputs);
            let keyed_signature_vector_wipoffset =
                build_keyed_signature_vector_wipoffset(builder, &tx.signatures);

            protocol::Transaction::create(
                builder,
                &protocol::TransactionArgs {
                    version: tx.version,
                    inputs: Some(input_vector_wipoffset),
                    outputs: Some(output_vector_wipoffset),
                    signatures: Some(keyed_signature_vector_wipoffset),
                },
            )
        })
        .collect();

    Some(builder.create_vector(&txns))
}

fn build_keyed_signature_vector_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    keyed_signature_vector: &[KeyedSignature],
) -> WOIPOffsetKeyedSignatureVector<'a> {
    let keyed_signature_vector_wipoffset: Vec<flatbuffers::WIPOffset<protocol::KeyedSignature>> =
        keyed_signature_vector
            .iter()
            .map(|keyed_signature: &KeyedSignature| {
                let signature_type = match keyed_signature.signature {
                    Signature::Secp256k1(_) => protocol::Signature::Secp256k1Signature,
                };
                let signature_wipoffset = match keyed_signature.signature {
                    Signature::Secp256k1(secp256k1) => {
                        let mut s = secp256k1.s.to_vec();
                        s.push(secp256k1.v);
                        let r_ftb = Some(builder.create_vector(&secp256k1.r));
                        let s_ftb = Some(builder.create_vector(&s));

                        protocol::Secp256k1Signature::create(
                            builder,
                            &protocol::Secp256k1SignatureArgs { r: r_ftb, s: s_ftb },
                        )
                        .as_union_value()
                    }
                };
                let publick_key_vector_wipoffset =
                    builder.create_vector(&keyed_signature.public_key);

                protocol::KeyedSignature::create(
                    builder,
                    &protocol::KeyedSignatureArgs {
                        signature_type,
                        signature: Some(signature_wipoffset),
                        public_key: Some(publick_key_vector_wipoffset),
                    },
                )
            })
            .collect();

    builder.create_vector(&keyed_signature_vector_wipoffset)
}

fn build_output_vector_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    output_vector: &[Output],
) -> WIPOffsetOutputVector<'a> {
    let output_vector_wipoffset: Vec<flatbuffers::WIPOffset<protocol::Output>> = output_vector
        .iter()
        .map(|output: &Output| match output {
            Output::Commit(commit) => {
                let commitment_wipoffset = build_hash_wipoffset(
                    builder,
                    &HashArgs {
                        hash: commit.commitment,
                    },
                );
                let commit_output_wipoffset = protocol::CommitOutput::create(
                    builder,
                    &protocol::CommitOutputArgs {
                        commitment: Some(commitment_wipoffset),
                        value: commit.value,
                    },
                );

                protocol::Output::create(
                    builder,
                    &protocol::OutputArgs {
                        output_type: protocol::OutputUnion::CommitOutput,
                        output: Some(commit_output_wipoffset.as_union_value()),
                    },
                )
            }
            Output::Consensus(consensus) => {
                let result_vector_wipoffset = builder.create_vector(&consensus.result);
                let pkh_wipoffset = build_hash_wipoffset(
                    builder,
                    &HashArgs {
                        hash: consensus.pkh,
                    },
                );
                let consensus_output_wipoffset = protocol::ConsensusOutput::create(
                    builder,
                    &protocol::ConsensusOutputArgs {
                        result: Some(result_vector_wipoffset),
                        pkh: Some(pkh_wipoffset),
                        value: consensus.value,
                    },
                );

                protocol::Output::create(
                    builder,
                    &protocol::OutputArgs {
                        output_type: protocol::OutputUnion::ConsensusOutput,
                        output: Some(consensus_output_wipoffset.as_union_value()),
                    },
                )
            }
            Output::DataRequest(data_request) => {
                let data_request_wipoffset = builder.create_vector(&data_request.data_request);
                let data_request_output_wipoffset = protocol::DataRequestOutput::create(
                    builder,
                    &protocol::DataRequestOutputArgs {
                        data_request: Some(data_request_wipoffset),
                        value: data_request.value,
                        witnesses: data_request.witnesses,
                        backup_witnesses: data_request.backup_witnesses,
                        commit_fee: data_request.commit_fee,
                        reveal_fee: data_request.reveal_fee,
                        tally_fee: data_request.tally_fee,
                        time_lock: data_request.time_lock,
                    },
                );

                protocol::Output::create(
                    builder,
                    &protocol::OutputArgs {
                        output_type: protocol::OutputUnion::DataRequestOutput,
                        output: Some(data_request_output_wipoffset.as_union_value()),
                    },
                )
            }
            Output::Reveal(reveal) => {
                let reveal_wipoffset = builder.create_vector(&reveal.reveal);
                let pkh_wipoffset = build_hash_wipoffset(builder, &HashArgs { hash: reveal.pkh });
                let reveal_output_wipoffset = protocol::RevealOutput::create(
                    builder,
                    &protocol::RevealOutputArgs {
                        reveal: Some(reveal_wipoffset),
                        pkh: Some(pkh_wipoffset),
                        value: reveal.value,
                    },
                );

                protocol::Output::create(
                    builder,
                    &protocol::OutputArgs {
                        output_type: protocol::OutputUnion::RevealOutput,
                        output: Some(reveal_output_wipoffset.as_union_value()),
                    },
                )
            }
            Output::ValueTransfer(value_transfer) => {
                let pkh_wipoffset = build_hash_wipoffset(
                    builder,
                    &HashArgs {
                        hash: value_transfer.pkh,
                    },
                );
                let value_transfer_wipoffset = protocol::ValueTransferOutput::create(
                    builder,
                    &protocol::ValueTransferOutputArgs {
                        pkh: Some(pkh_wipoffset),
                        value: value_transfer.value,
                    },
                );

                protocol::Output::create(
                    builder,
                    &protocol::OutputArgs {
                        output_type: protocol::OutputUnion::ValueTransferOutput,
                        output: Some(value_transfer_wipoffset.as_union_value()),
                    },
                )
            }
        })
        .collect();

    builder.create_vector(&output_vector_wipoffset)
}

fn build_input_vector_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    input_vector: &[Input],
) -> WIPOffsetInputVector<'a> {
    let input_vector_wipoffset: Vec<flatbuffers::WIPOffset<protocol::Input>> = input_vector
        .iter()
        .map(|input: &Input| match input {
            Input::ValueTransfer(value_transfer) => {
                let transaction_id = builder.create_vector(&value_transfer.transaction_id);
                let value_transfer_input_wipoffset = protocol::ValueTransferInput::create(
                    builder,
                    &protocol::ValueTransferInputArgs {
                        output_index: value_transfer.output_index,
                        transaction_id: Some(transaction_id),
                    },
                );

                protocol::Input::create(
                    builder,
                    &protocol::InputArgs {
                        input_type: protocol::InputUnion::ValueTransferInput,
                        input: Some(value_transfer_input_wipoffset.as_union_value()),
                    },
                )
            }
            Input::Commit(commit) => {
                let transaction_id = builder.create_vector(&commit.transaction_id);
                let poe = builder.create_vector(&commit.poe);
                let commit_input_wipoffset = protocol::CommitInput::create(
                    builder,
                    &protocol::CommitInputArgs {
                        transaction_id: Some(transaction_id),
                        output_index: commit.output_index,
                        poe: Some(poe),
                    },
                );

                protocol::Input::create(
                    builder,
                    &protocol::InputArgs {
                        input_type: protocol::InputUnion::CommitInput,
                        input: Some(commit_input_wipoffset.as_union_value()),
                    },
                )
            }
            Input::Reveal(reveal) => {
                let transaction_id_vector_wipoffset = builder.create_vector(&reveal.transaction_id);
                let reveal_vector_wipoffset = builder.create_vector(&reveal.reveal);
                let reveal_input_wipoffset = protocol::RevealInput::create(
                    builder,
                    &protocol::RevealInputArgs {
                        transaction_id: Some(transaction_id_vector_wipoffset),
                        output_index: reveal.output_index,
                        reveal: Some(reveal_vector_wipoffset),
                        nonce: reveal.nonce,
                    },
                );

                protocol::Input::create(
                    builder,
                    &protocol::InputArgs {
                        input_type: protocol::InputUnion::RevealInput,
                        input: Some(reveal_input_wipoffset.as_union_value()),
                    },
                )
            }
            Input::Tally(tally) => {
                let transaction_id_vector_wipoffset = builder.create_vector(&tally.transaction_id);
                let tally_input_wipoffset = protocol::TallyInput::create(
                    builder,
                    &protocol::TallyInputArgs {
                        transaction_id: Some(transaction_id_vector_wipoffset),
                        output_index: tally.output_index,
                    },
                );
                protocol::Input::create(
                    builder,
                    &protocol::InputArgs {
                        input_type: protocol::InputUnion::TallyInput,
                        input: Some(tally_input_wipoffset.as_union_value()),
                    },
                )
            }
        })
        .collect();

    builder.create_vector(&input_vector_wipoffset)
}

pub fn build_verack_message_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
) -> WIPOffsetVerackMessage<'a> {
    protocol::Verack::create(builder, &protocol::VerackArgs {})
}

pub fn build_version_message_wipoffset<'a>(
    builder: &mut FlatBufferBuilder<'a>,
    version_message_args: &VersionMessageArgs,
) -> WIPOffsetVersionMessage<'a> {
    let sender_address_wipoffset = build_address_wipoffset(
        builder,
        &AddressArgs {
            ip: version_message_args.sender_ip,
            port: version_message_args.sender_port,
        },
    );
    let receiver_address_wipoffset = build_address_wipoffset(
        builder,
        &AddressArgs {
            ip: version_message_args.receiver_ip,
            port: version_message_args.receiver_port,
        },
    );
    let user_agent = builder.create_string(&version_message_args.user_agent);

    protocol::Version::create(
        builder,
        &protocol::VersionArgs {
            version: version_message_args.version,
            timestamp: version_message_args.timestamp,
            capabilities: version_message_args.capabilities,
            sender_address: Some(sender_address_wipoffset),
            receiver_address: Some(receiver_address_wipoffset),
            user_agent: Some(user_agent),
            last_epoch: version_message_args.last_epoch,
            genesis: version_message_args.genesis,
            nonce: version_message_args.nonce,
        },
    )
}
