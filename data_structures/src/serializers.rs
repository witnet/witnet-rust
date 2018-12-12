extern crate flatbuffers;

use std::convert::Into;

use crate::chain::{
    Block, BlockHeader, CheckpointBeacon, Hash, InventoryEntry, LeadershipProof,
    Secp256k1Signature, Signature, Transaction, SHA256,
};
use crate::flatbuffers::protocol_generated::protocol;

use crate::types::{
    Address, Command, GetPeers, InventoryAnnouncement, InventoryRequest,
    IpAddress::{Ipv4, Ipv6},
    LastBeacon, Message, Peers, Ping, Pong, Verack, Version,
};

use flatbuffers::FlatBufferBuilder;

const FTB_SIZE: usize = 1024;

////////////////////////////////////////////////////////
// COMMAND ARGS
////////////////////////////////////////////////////////
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
struct BlockCommandArgs<'a> {
    magic: u16,
    block_header: BlockHeader,
    proof: LeadershipProof,
    txns: &'a [Transaction],
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

////////////////////////////////////////////////////////
// FROM TRAIT (Vec<u8> ---> Message)
////////////////////////////////////////////////////////
pub trait TryFrom<T>: Sized {
    type Error;

    fn try_from(value: T) -> Result<Self, Self::Error>;
}

impl TryFrom<Vec<u8>> for Message {
    type Error = &'static str;

    fn try_from(bytes: Vec<u8>) -> Result<Self, &'static str> {
        // Get flatbuffers Message
        let message = protocol::get_root_as_message(&bytes);

        // Get magic field from message
        let magic = message.magic();

        // Create Witnet's message to decode a flatbuffer message
        match message.command_type() {
            // Heartbeat
            protocol::Command::Ping => message
                .command_as_ping()
                .map(|ping| {
                    create_ping_message(HeartbeatCommandsArgs {
                        nonce: ping.nonce(),
                        magic,
                    })
                })
                .ok_or(""),
            protocol::Command::Pong => message
                .command_as_pong()
                .map(|pong| {
                    create_pong_message(HeartbeatCommandsArgs {
                        nonce: pong.nonce(),
                        magic,
                    })
                })
                .ok_or(""),

            // Peer discovery
            protocol::Command::GetPeers => Ok(create_get_peers_message(EmptyCommandArgs { magic })),
            protocol::Command::Peers => message
                .command_as_peers()
                .and_then(|peers| create_peers_message(PeersWitnetArgs { magic, peers }))
                .ok_or(""),

            // Handshake
            protocol::Command::Verack => Ok(create_verack_message(EmptyCommandArgs { magic })),
            protocol::Command::Version => message
                .command_as_version()
                .and_then(|command| {
                    // Get ftb addresses and create Witnet addresses
                    let sender_address = command.sender_address().and_then(create_address);
                    let receiver_address = command.receiver_address().and_then(create_address);

                    // Check if sender address and receiver address exist
                    if sender_address.and(receiver_address).is_some() {
                        Some(create_version_message(VersionCommandArgs {
                            version: command.version(),
                            timestamp: command.timestamp(),
                            capabilities: command.capabilities(),
                            sender_address: &sender_address?,
                            receiver_address: &receiver_address?,
                            user_agent: &command.user_agent().to_string(),
                            last_epoch: command.last_epoch(),
                            genesis: command.genesis(),
                            nonce: command.nonce(),
                            magic,
                        }))
                    } else {
                        None
                    }
                })
                .ok_or(""),

            // Inventory
            protocol::Command::Block => message
                .command_as_block()
                .map(|block| {
                    // Get Header
                    let block_header = block.block_header();
                    // Get transactions
                    let len = block.txns().len();
                    let mut counter = 0;
                    let mut _tx_ftb;
                    let mut block_txns = Vec::new();
                    while counter < len {
                        _tx_ftb = block.txns().get(counter);
                        // Call create_transaction(ftb_tx) in order to get native Transaction
                        block_txns.push(Transaction);
                        counter += 1;
                    }

                    let version = block_header.version();
                    // Get CheckpointBeacon
                    let hash: Hash = match block_header.beacon().hash_prev_block().type_() {
                        protocol::HashType::SHA256 => {
                            let mut sha256: SHA256 = [0; 32];
                            let sha256_bytes = block_header.beacon().hash_prev_block().bytes();
                            sha256.copy_from_slice(sha256_bytes);

                            Hash::SHA256(sha256)
                        }
                    };
                    let beacon = CheckpointBeacon {
                        checkpoint: block_header.beacon().checkpoint(),
                        hash_prev_block: hash,
                    };
                    // Get hash merkle root
                    let hash_merkle_root: Hash = match block_header.hash_merkle_root().type_() {
                        protocol::HashType::SHA256 => {
                            let mut sha256: SHA256 = [0; 32];
                            let sha256_bytes = block_header.hash_merkle_root().bytes();
                            sha256.copy_from_slice(sha256_bytes);

                            Hash::SHA256(sha256)
                        }
                    };
                    // Get proof of leadership
                    let block_sig = match block.proof().block_sig_type() {
                        protocol::Signature::Secp256k1Signature => block
                            .proof()
                            .block_sig_as_secp_256k_1signature()
                            .and_then(|signature_ftb| {
                                let mut signature = Secp256k1Signature {
                                    r: [0; 32],
                                    s: [0; 32],
                                    v: 0,
                                };
                                signature.r.copy_from_slice(&signature_ftb.r()[0..32]);
                                signature.s.copy_from_slice(&signature_ftb.s()[0..32]);
                                signature.v = signature_ftb.s()[32];

                                Some(Signature::Secp256k1(signature))
                            }),
                        _ => None,
                    };
                    let influence = block.proof().influence();
                    let proof = LeadershipProof {
                        block_sig,
                        influence,
                    };
                    // Create BlockHeader
                    let header = BlockHeader {
                        version,
                        beacon,
                        hash_merkle_root,
                    };

                    // Create Message with command
                    Message {
                        kind: Command::Block(Block {
                            block_header: header,
                            proof,
                            txns: block_txns,
                        }),
                        magic,
                    }
                })
                .ok_or(""),
            protocol::Command::InventoryAnnouncement => message
                .command_as_inventory_announcement()
                .and_then(|inventory| {
                    Some(create_inventory_announcement_message(
                        InventoryAnnouncementWitnetArgs { magic, inventory },
                    ))
                })
                .ok_or(""),
            protocol::Command::InventoryRequest => message
                .command_as_inventory_request()
                .and_then(|inventory| {
                    Some(create_inventory_request_message(
                        InventoryRequestWitnetArgs { magic, inventory },
                    ))
                })
                .ok_or(""),
            protocol::Command::LastBeacon => message
                .command_as_last_beacon()
                .map(|last_beacon| {
                    let hash_prev_block = match last_beacon
                        .highest_block_checkpoint()
                        .hash_prev_block()
                        .type_()
                    {
                        protocol::HashType::SHA256 => {
                            let mut sha256: SHA256 = [0; 32];
                            let sha256_bytes = last_beacon
                                .highest_block_checkpoint()
                                .hash_prev_block()
                                .bytes();
                            sha256.copy_from_slice(sha256_bytes);
                            Hash::SHA256(sha256)
                        }
                    };
                    let highest_block_checkpoint = CheckpointBeacon {
                        checkpoint: last_beacon.highest_block_checkpoint().checkpoint(),
                        hash_prev_block,
                    };
                    create_last_beacon_message(LastBeaconCommandArgs {
                        highest_block_checkpoint,
                        magic,
                    })
                })
                .ok_or(""),

            // No command
            protocol::Command::NONE => Err(""),
        }
    }
}

////////////////////////////////////////////////////////
// INTO TRAIT (Message ----> Vec<u8>)
////////////////////////////////////////////////////////
impl Into<Vec<u8>> for Message {
    fn into(self) -> Vec<u8> {
        // Create builder to create flatbuffers to encode Witnet messages
        let mut builder = flatbuffers::FlatBufferBuilder::new_with_capacity(FTB_SIZE);

        // Create flatbuffer to encode a Witnet message
        match self.kind {
            // Heartbeat
            Command::Ping(Ping { nonce }) => create_ping_flatbuffer(
                &mut builder,
                HeartbeatCommandsArgs {
                    magic: self.magic,
                    nonce,
                },
            ),
            Command::Pong(Pong { nonce }) => create_pong_flatbuffer(
                &mut builder,
                HeartbeatCommandsArgs {
                    magic: self.magic,
                    nonce,
                },
            ),

            // Peer discovery
            Command::GetPeers(GetPeers) => {
                create_get_peers_flatbuffer(&mut builder, EmptyCommandArgs { magic: self.magic })
            }
            Command::Peers(Peers { peers }) => create_peers_flatbuffer(
                &mut builder,
                PeersFlatbufferArgs {
                    magic: self.magic,
                    peers: &peers,
                },
            ),

            // Handshake
            Command::Verack(Verack) => {
                create_verack_flatbuffer(&mut builder, EmptyCommandArgs { magic: self.magic })
            }
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
            }) => create_version_flatbuffer(
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

            // Inventory
            Command::Block(Block {
                block_header,
                proof,
                txns,
            }) => create_block_flatbuffer(
                &mut builder,
                BlockCommandArgs {
                    magic: self.magic,
                    block_header,
                    proof,
                    txns: &txns,
                },
            ),
            Command::InventoryAnnouncement(InventoryAnnouncement { inventory }) => {
                create_inventory_announcement_flatbuffer(
                    &mut builder,
                    InventoryArgs {
                        magic: self.magic,
                        inventory: &inventory,
                    },
                )
            }
            Command::InventoryRequest(InventoryRequest { inventory }) => {
                create_inventory_request_flatbuffer(
                    &mut builder,
                    InventoryArgs {
                        magic: self.magic,
                        inventory: &inventory,
                    },
                )
            }
            Command::LastBeacon(LastBeacon {
                highest_block_checkpoint,
            }) => create_last_beacon_flatbuffer(
                &mut builder,
                LastBeaconCommandArgs {
                    magic: self.magic,
                    highest_block_checkpoint,
                },
            ),
        }
    }
}

////////////////////////////////////////////////////////
// FROM TRAIT AUX FUNCTIONS: to create Witnet's types
////////////////////////////////////////////////////////
// Create a Witnet Ping message to decode a flatbuffers' Ping message
fn create_ping_message(ping_args: HeartbeatCommandsArgs) -> Message {
    Message {
        kind: Command::Ping(Ping {
            nonce: ping_args.nonce,
        }),
        magic: ping_args.magic,
    }
}

// Create a Witnet Pong message to decode a flatbuffers' Pong message
fn create_pong_message(pong_args: HeartbeatCommandsArgs) -> Message {
    Message {
        kind: Command::Pong(Pong {
            nonce: pong_args.nonce,
        }),
        magic: pong_args.magic,
    }
}

// Create a Witnet GetPeers message to decode a flatbuffers' GetPeers message
fn create_get_peers_message(get_peers_args: EmptyCommandArgs) -> Message {
    Message {
        kind: Command::GetPeers(GetPeers),
        magic: get_peers_args.magic,
    }
}

// Create a Witnet's Peers message to decode a flatbuffers' Peers message
fn create_peers_message(peers_args: PeersWitnetArgs) -> Option<Message> {
    peers_args.peers.peers().map(|ftb_addresses| {
        // TODO: Refactor as declarative code [24-10-2018]
        let len = ftb_addresses.len();
        let mut counter = 0;
        let mut ftb_address: Option<Address>;
        let mut peer;
        let mut vec_addresses = Vec::new();
        while counter < len {
            peer = ftb_addresses.get(counter);
            ftb_address = create_address(peer);
            if ftb_address.is_some() {
                vec_addresses.push(ftb_address.unwrap());
            }
            counter += 1;
        }
        Message {
            kind: Command::Peers(Peers {
                peers: vec_addresses,
            }),
            magic: peers_args.magic,
        }
    })
}

// Create a Witnet Verack message to decode a flatbuffers' Verack message
fn create_verack_message(verack_args: EmptyCommandArgs) -> Message {
    Message {
        kind: Command::Verack(Verack),
        magic: verack_args.magic,
    }
}

// Create a Witnet Version message to decode a flatbuffers' Version message
fn create_version_message(version_args: VersionCommandArgs) -> Message {
    Message {
        kind: Command::Version(Version {
            version: version_args.version,
            timestamp: version_args.timestamp,
            capabilities: version_args.capabilities,
            sender_address: *version_args.sender_address,
            receiver_address: *version_args.receiver_address,
            user_agent: version_args.user_agent.to_string(),
            last_epoch: version_args.last_epoch,
            genesis: version_args.genesis,
            nonce: version_args.nonce,
        }),
        magic: version_args.magic,
    }
}

// Create a Witnet's InventoryAnnouncement message to decode a flatbuffers' InventoryAnnouncement
// message
fn create_inventory_announcement_message(inv_args: InventoryAnnouncementWitnetArgs) -> Message {
    // Get inventory entries (flatbuffers' types)
    let ftb_inv_items = inv_args.inventory.inventory();
    let len = ftb_inv_items.len();

    // Create empty vector of inventory entries
    let mut inv_items = Vec::new();

    // Create all inventory entries (Witnet's types) and add them to a vector
    for i in 0..len {
        let inv_item = create_inventory_entry(ftb_inv_items.get(i));
        inv_items.push(inv_item);
    }

    // Create message
    Message {
        magic: inv_args.magic,
        kind: Command::InventoryAnnouncement(InventoryAnnouncement {
            inventory: inv_items,
        }),
    }
}

// Create a Witnet's InventoryRequest message to decode a flatbuffers' InventoryRequest message
fn create_inventory_request_message(get_data_args: InventoryRequestWitnetArgs) -> Message {
    // Get inventory entries (flatbuffers' types)
    let ftb_inv_items = get_data_args.inventory.inventory();
    let len = ftb_inv_items.len();

    // Create empty vector of inventory entries
    let mut inv_items = Vec::new();

    // Create all inventory entries (Witnet's types) and add them to a vector
    for i in 0..len {
        let inv_item = create_inventory_entry(ftb_inv_items.get(i));
        inv_items.push(inv_item);
    }

    // Create message
    Message {
        magic: get_data_args.magic,
        kind: Command::InventoryRequest(InventoryRequest {
            inventory: inv_items,
        }),
    }
}

// Create a Witnet LastBeacon message to decode flatbuffers' LastBeacon message
fn create_last_beacon_message(last_beacon_args: LastBeaconCommandArgs) -> Message {
    Message {
        kind: Command::LastBeacon(LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                checkpoint: last_beacon_args.highest_block_checkpoint.checkpoint,
                hash_prev_block: last_beacon_args.highest_block_checkpoint.hash_prev_block,
            },
        }),
        magic: last_beacon_args.magic,
    }
}

// Create a Witnet's InventoryEntry from a flatbuffers' InventoryEntry
fn create_inventory_entry(inv_item: protocol::InventoryEntry) -> InventoryEntry {
    // Create inventory entry hash
    let hash = create_hash(inv_item.hash());

    // Create inventory entry
    match inv_item.type_() {
        protocol::InventoryItemType::Error => InventoryEntry::Error(hash),
        protocol::InventoryItemType::Tx => InventoryEntry::Tx(hash),
        protocol::InventoryItemType::Block => InventoryEntry::Block(hash),
        protocol::InventoryItemType::DataRequest => InventoryEntry::DataRequest(hash),
        protocol::InventoryItemType::DataResult => InventoryEntry::DataResult(hash),
    }
}

// Create a Witnet's Hash from a flatbuffers' Hash
fn create_hash(hash: protocol::Hash) -> Hash {
    // Get hash bytes
    let mut hash_bytes: SHA256 = [0; 32];
    hash_bytes.copy_from_slice(hash.bytes());

    // Build hash
    match hash.type_() {
        protocol::HashType::SHA256 => Hash::SHA256(hash_bytes),
    }
}

// Create Witnet IP address
fn create_address(address: protocol::Address) -> Option<Address> {
    match address.ip_type() {
        protocol::IpAddress::Ipv4 => address
            .ip_as_ipv_4()
            .map(|ipv4| create_ipv4_address(ipv4.ip(), address.port())),
        protocol::IpAddress::Ipv6 => match address.ip_as_ipv_6() {
            Some(hextets) => Some(create_ipv6_address(
                hextets.ip0(),
                hextets.ip1(),
                hextets.ip2(),
                hextets.ip3(),
                address.port(),
            )),
            None => None,
        },
        protocol::IpAddress::NONE => None,
    }
}

// Create Witnet IPv4 address
fn create_ipv4_address(ip: u32, port: u16) -> Address {
    Address {
        ip: Ipv4 { ip },
        port,
    }
}

// Create Witnet IPv6 address
fn create_ipv6_address(ip0: u32, ip1: u32, ip2: u32, ip3: u32, port: u16) -> Address {
    Address {
        ip: Ipv6 { ip0, ip1, ip2, ip3 },
        port,
    }
}

////////////////////////////////////////////////////////
// INTO TRAIT AUX FUNCTIONS: to create ftb types
////////////////////////////////////////////////////////
// Convert a flatbuffers message into a vector of bytes
fn build_flatbuffer(
    builder: &mut FlatBufferBuilder,
    message: flatbuffers::WIPOffset<protocol::Message>,
) -> Vec<u8> {
    builder.finish(message, None);
    builder.finished_data().to_vec()
}

// Create a Ping flatbuffer to encode a Witnet's Ping message
fn create_ping_flatbuffer(
    builder: &mut FlatBufferBuilder,
    ping_args: HeartbeatCommandsArgs,
) -> Vec<u8> {
    let ping_command = protocol::Ping::create(
        builder,
        &protocol::PingArgs {
            nonce: ping_args.nonce.to_owned(),
        },
    );
    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: ping_args.magic,
            command_type: protocol::Command::Ping,
            command: Some(ping_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create a Pong flatbuffer to encode a Witnet's Pong message
fn create_pong_flatbuffer(
    builder: &mut FlatBufferBuilder,
    pong_args: HeartbeatCommandsArgs,
) -> Vec<u8> {
    let pong_command = protocol::Pong::create(
        builder,
        &protocol::PongArgs {
            nonce: pong_args.nonce,
        },
    );
    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: pong_args.magic,
            command_type: protocol::Command::Pong,
            command: Some(pong_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create a GetPeers flatbuffer to encode Witnet's GetPeers message
fn create_get_peers_flatbuffer(
    builder: &mut FlatBufferBuilder,
    get_peers_args: EmptyCommandArgs,
) -> Vec<u8> {
    let get_peers_command = protocol::GetPeers::create(builder, &protocol::GetPeersArgs {});

    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: get_peers_args.magic,
            command_type: protocol::Command::GetPeers,
            command: Some(get_peers_command.as_union_value()),
        },
    );
    build_flatbuffer(builder, message)
}

// Create a Peers flatbuffer to encode a Witnet's Peers message
fn create_peers_flatbuffer(
    builder: &mut FlatBufferBuilder,
    peers_args: PeersFlatbufferArgs,
) -> Vec<u8> {
    let addresses_command: Vec<flatbuffers::WIPOffset<protocol::Address>> = peers_args
        .peers
        .iter()
        .map(|peer: &Address| match peer.ip {
            Ipv4 { ip } => {
                let ip_command = protocol::Ipv4::create(builder, &protocol::Ipv4Args { ip });
                protocol::Address::create(
                    builder,
                    &protocol::AddressArgs {
                        ip_type: protocol::IpAddress::Ipv4,
                        ip: Some(ip_command.as_union_value()),
                        port: peer.port,
                    },
                )
            }
            Ipv6 { ip0, ip1, ip2, ip3 } => {
                let ip_command =
                    protocol::Ipv6::create(builder, &protocol::Ipv6Args { ip0, ip1, ip2, ip3 });
                protocol::Address::create(
                    builder,
                    &protocol::AddressArgs {
                        ip_type: protocol::IpAddress::Ipv6,
                        ip: Some(ip_command.as_union_value()),
                        port: peer.port,
                    },
                )
            }
        })
        .collect();

    let addresses = Some(builder.create_vector(&addresses_command));
    let peers_command = protocol::Peers::create(builder, &protocol::PeersArgs { peers: addresses });

    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: peers_args.magic,
            command_type: protocol::Command::Peers,
            command: Some(peers_command.as_union_value()),
        },
    );
    build_flatbuffer(builder, message)
}

// Create a Verack flatbuffer to encode a Witnet's Verack message
fn create_verack_flatbuffer(
    builder: &mut FlatBufferBuilder,
    verack_args: EmptyCommandArgs,
) -> Vec<u8> {
    let verack_command = protocol::Verack::create(builder, &protocol::VerackArgs {});

    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: verack_args.magic,
            command_type: protocol::Command::Verack,
            command: Some(verack_command.as_union_value()),
        },
    );
    build_flatbuffer(builder, message)
}

// Create a Version flatbuffer to encode a Witnet's Version message
fn create_version_flatbuffer(
    builder: &mut FlatBufferBuilder,
    version_args: VersionCommandArgs,
) -> Vec<u8> {
    let sender_address_command = match version_args.sender_address.ip {
        Ipv4 { ip } => {
            let ip_command = protocol::Ipv4::create(builder, &protocol::Ipv4Args { ip });
            protocol::Address::create(
                builder,
                &protocol::AddressArgs {
                    ip_type: protocol::IpAddress::Ipv4,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.sender_address.port,
                },
            )
        }
        Ipv6 { ip0, ip1, ip2, ip3 } => {
            let ip_command =
                protocol::Ipv6::create(builder, &protocol::Ipv6Args { ip0, ip1, ip2, ip3 });
            protocol::Address::create(
                builder,
                &protocol::AddressArgs {
                    ip_type: protocol::IpAddress::Ipv6,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.sender_address.port,
                },
            )
        }
    };

    let receiver_address_command = match version_args.receiver_address.ip {
        Ipv4 { ip } => {
            let ip_command = protocol::Ipv4::create(builder, &protocol::Ipv4Args { ip });
            protocol::Address::create(
                builder,
                &protocol::AddressArgs {
                    ip_type: protocol::IpAddress::Ipv4,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.receiver_address.port,
                },
            )
        }
        Ipv6 { ip0, ip1, ip2, ip3 } => {
            let ip_command =
                protocol::Ipv6::create(builder, &protocol::Ipv6Args { ip0, ip1, ip2, ip3 });
            protocol::Address::create(
                builder,
                &protocol::AddressArgs {
                    ip_type: protocol::IpAddress::Ipv6,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.receiver_address.port,
                },
            )
        }
    };

    let user_agent = Some(builder.create_string(&version_args.user_agent));
    let version_command = protocol::Version::create(
        builder,
        &protocol::VersionArgs {
            version: version_args.version,
            timestamp: version_args.timestamp,
            capabilities: version_args.capabilities,
            sender_address: Some(sender_address_command),
            receiver_address: Some(receiver_address_command),
            user_agent,
            last_epoch: version_args.last_epoch,
            genesis: version_args.genesis,
            nonce: version_args.nonce,
        },
    );

    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: version_args.magic,
            command_type: protocol::Command::Version,
            command: Some(version_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create a Block flatbuffer to encode a Witnet's Block message
fn create_block_flatbuffer(
    builder: &mut FlatBufferBuilder,
    block_args: BlockCommandArgs,
) -> Vec<u8> {
    // Create checkpoint beacon flatbuffer
    let hash_prev_block_args = match block_args.block_header.beacon.hash_prev_block {
        Hash::SHA256(hash) => protocol::HashArgs {
            type_: protocol::HashType::SHA256,
            bytes: Some(builder.create_vector(&hash)),
        },
    };
    let hash_prev_block = Some(protocol::Hash::create(builder, &hash_prev_block_args));
    let beacon = Some(protocol::CheckpointBeacon::create(
        builder,
        &protocol::CheckpointBeaconArgs {
            checkpoint: block_args.block_header.beacon.checkpoint,
            hash_prev_block,
        },
    ));
    // Create hash merkle root flatbuffer
    let hash_merkle_root_args = match block_args.block_header.hash_merkle_root {
        Hash::SHA256(hash) => protocol::HashArgs {
            type_: protocol::HashType::SHA256,
            bytes: Some(builder.create_vector(&hash)),
        },
    };
    let hash_merkle_root = Some(protocol::Hash::create(builder, &hash_merkle_root_args));
    // Create proof of leadership flatbuffer
    let block_sig_type = block_args
        .proof
        .block_sig
        .clone()
        .map(|signature| match signature {
            Signature::Secp256k1(_) => protocol::Signature::Secp256k1Signature,
        });
    let block_sig = block_args.proof.block_sig.map(|signature| match signature {
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
    let proof = Some(protocol::LeadershipProof::create(
        builder,
        &protocol::LeadershipProofArgs {
            block_sig_type: block_sig_type.unwrap_or(protocol::Signature::NONE),
            block_sig,
            influence: block_args.proof.influence,
        },
    ));
    // Create block header flatbuffer
    let block_header = Some(protocol::BlockHeader::create(
        builder,
        &protocol::BlockHeaderArgs {
            version: block_args.block_header.version,
            beacon,
            hash_merkle_root,
        },
    ));
    // Create transaction array flatbuffer
    let txns: Vec<flatbuffers::WIPOffset<protocol::Transaction>> = block_args
        .txns
        .iter()
        .map(|_tx: &Transaction| {
            protocol::Transaction::create(builder, &protocol::TransactionArgs {})
        })
        .collect();
    let txns_ftb = Some(builder.create_vector(&txns));
    // Create block command flatbuffer
    let block_command = protocol::Block::create(
        builder,
        &protocol::BlockArgs {
            block_header,
            proof,
            txns: txns_ftb,
        },
    );
    // Create message flatbuffer
    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: block_args.magic,
            command_type: protocol::Command::Block,
            command: Some(block_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create an InventoryAnnouncement flatbuffer to encode a Witnet's InventoryAnnouncement message
fn create_inventory_announcement_flatbuffer(
    builder: &mut FlatBufferBuilder,
    inv_args: InventoryArgs,
) -> Vec<u8> {
    // Create vector of flatbuffers' inv items
    let ftb_inv_items: Vec<flatbuffers::WIPOffset<protocol::InventoryEntry>> = inv_args
        .inventory
        .iter()
        .map(|inv_item: &InventoryEntry| {
            // Create flatbuffers' hash bytes
            let hash = match inv_item {
                InventoryEntry::Error(hash)
                | InventoryEntry::Tx(hash)
                | InventoryEntry::Block(hash)
                | InventoryEntry::DataRequest(hash)
                | InventoryEntry::DataResult(hash) => hash,
            };

            // Get hash bytes
            let bytes = match hash {
                Hash::SHA256(bytes) => builder.create_vector(bytes),
            };

            // Create flatbuffers' hash
            let ftb_hash = match hash {
                Hash::SHA256(_) => protocol::Hash::create(
                    builder,
                    &protocol::HashArgs {
                        type_: protocol::HashType::SHA256,
                        bytes: Some(bytes),
                    },
                ),
            };

            // Create flatbuffers inv vector type
            let ftb_type = match inv_item {
                InventoryEntry::Error(_) => protocol::InventoryItemType::Error,
                InventoryEntry::Tx(_) => protocol::InventoryItemType::Tx,
                InventoryEntry::Block(_) => protocol::InventoryItemType::Block,
                InventoryEntry::DataRequest(_) => protocol::InventoryItemType::DataRequest,
                InventoryEntry::DataResult(_) => protocol::InventoryItemType::DataResult,
            };

            // Create flatbuffers inv vector
            protocol::InventoryEntry::create(
                builder,
                &protocol::InventoryEntryArgs {
                    type_: ftb_type,
                    hash: Some(ftb_hash),
                },
            )
        })
        .collect();

    // Create flatbuffers' vector of flatbuffers' inv items
    let ftb_inv_items = Some(builder.create_vector(&ftb_inv_items));

    // Create inv flatbuffers command
    let inv_command = protocol::InventoryAnnouncement::create(
        builder,
        &protocol::InventoryAnnouncementArgs {
            inventory: ftb_inv_items,
        },
    );

    // Create flatbuffers message
    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: inv_args.magic,
            command_type: protocol::Command::InventoryAnnouncement,
            command: Some(inv_command.as_union_value()),
        },
    );

    // Get vector of bytes from flatbuffer message
    build_flatbuffer(builder, message)
}

// Create an InventoryRequest flatbuffer to encode a Witnet's InventoryRequest message
fn create_inventory_request_flatbuffer(
    builder: &mut FlatBufferBuilder,
    get_data_args: InventoryArgs,
) -> Vec<u8> {
    // Create vector of flatbuffers' inv items
    let ftb_inv_items: Vec<flatbuffers::WIPOffset<protocol::InventoryEntry>> = get_data_args
        .inventory
        .iter()
        .map(|inv_item: &InventoryEntry| {
            // Create flatbuffers' hash bytes
            let hash = match inv_item {
                InventoryEntry::Error(hash)
                | InventoryEntry::Tx(hash)
                | InventoryEntry::Block(hash)
                | InventoryEntry::DataRequest(hash)
                | InventoryEntry::DataResult(hash) => hash,
            };

            // Get hash bytes
            let bytes = match hash {
                Hash::SHA256(bytes) => builder.create_vector(bytes),
            };

            // Create flatbuffers' hash
            let ftb_hash = match hash {
                Hash::SHA256(_) => protocol::Hash::create(
                    builder,
                    &protocol::HashArgs {
                        type_: protocol::HashType::SHA256,
                        bytes: Some(bytes),
                    },
                ),
            };

            // Create flatbuffers inv item type
            let ftb_type = match inv_item {
                InventoryEntry::Error(_) => protocol::InventoryItemType::Error,
                InventoryEntry::Tx(_) => protocol::InventoryItemType::Tx,
                InventoryEntry::Block(_) => protocol::InventoryItemType::Block,
                InventoryEntry::DataRequest(_) => protocol::InventoryItemType::DataRequest,
                InventoryEntry::DataResult(_) => protocol::InventoryItemType::DataResult,
            };

            // Create flatbuffers inv item
            protocol::InventoryEntry::create(
                builder,
                &protocol::InventoryEntryArgs {
                    type_: ftb_type,
                    hash: Some(ftb_hash),
                },
            )
        })
        .collect();

    // Create flatbuffers' vector of flatbuffers' inv items
    let ftb_inv_items = Some(builder.create_vector(&ftb_inv_items));

    // Create get_data flatbuffers command
    let get_data_command = protocol::InventoryRequest::create(
        builder,
        &protocol::InventoryRequestArgs {
            inventory: ftb_inv_items,
        },
    );

    // Create flatbuffers message
    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: get_data_args.magic,
            command_type: protocol::Command::InventoryRequest,
            command: Some(get_data_command.as_union_value()),
        },
    );

    // Get vector of bytes from flatbuffer message
    build_flatbuffer(builder, message)
}

// Create a LastBeacon flatbuffer to encode a Witnet LastBeacon message
fn create_last_beacon_flatbuffer(
    builder: &mut FlatBufferBuilder,
    last_beacon_args: LastBeaconCommandArgs,
) -> Vec<u8> {
    let Hash::SHA256(hash) = last_beacon_args.highest_block_checkpoint.hash_prev_block;
    let ftb_hash = builder.create_vector(&hash);
    let hash_command = protocol::Hash::create(
        builder,
        &protocol::HashArgs {
            type_: protocol::HashType::SHA256,
            bytes: Some(ftb_hash),
        },
    );

    let beacon = protocol::CheckpointBeacon::create(
        builder,
        &protocol::CheckpointBeaconArgs {
            checkpoint: last_beacon_args.highest_block_checkpoint.checkpoint,
            hash_prev_block: Some(hash_command),
        },
    );

    let last_beacon_command = protocol::LastBeacon::create(
        builder,
        &protocol::LastBeaconArgs {
            highest_block_checkpoint: Some(beacon),
        },
    );
    let message = protocol::Message::create(
        builder,
        &protocol::MessageArgs {
            magic: last_beacon_args.magic,
            command_type: protocol::Command::LastBeacon,
            command: Some(last_beacon_command.as_union_value()),
        },
    );
    build_flatbuffer(builder, message)
}
