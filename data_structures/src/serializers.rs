extern crate flatbuffers;

use std::convert::Into;

use crate::chain::{
    Block, BlockHeader, BlockHeaderWithProof, CheckpointBeacon, Hash, InvVector, LeadershipProof,
    Secp256k1Signature, Signature, Transaction, SHA256,
};
use crate::flatbuffers::protocol_generated::protocol::{
    get_root_as_message, Address as FlatBufferAddress, AddressArgs, Block as FlatBuffersBlock,
    BlockArgs, BlockHeader as FlatBuffersBlockHeader, BlockHeaderArgs,
    CheckpointBeacon as FlatBuffersCheckpointBeacon,
    CheckpointBeaconArgs as FlatBuffersCheckpointBeaconArgs, Command as FlatBuffersCommand,
    GetPeers as FlatBuffersGetPeers, GetPeersArgs, Hash as FlatBuffersHash, HashArgs,
    HashType as FlatBuffersHashType, Inv as FlatBuffersInv, InvArgs,
    GetData as FlatBuffersGetData, GetDataArgs, GetPeers as FlatBuffersGetPeers, GetPeersArgs,
    Hash as FlatBuffersHash, HashArgs, HashType as FlatBuffersHashType, Inv as FlatBuffersInv,
    InvArgs, InvVector as FlatBuffersInvVector, InvVectorArgs, InvVectorType as FlatBuffersInvVectorType,
    IpAddress as FlatBuffersIpAddress, Ipv4 as FlatBuffersIpv4, Ipv4Args as FlatBuffersIpv4Args,
    Ipv6 as FlatBuffersIpv6, Ipv6Args as FlatBuffersIpv6Args,
    LeadershipProof as FlatBuffersLeadershipProof, LeadershipProofArgs,
    Message as FlatBuffersMessage, MessageArgs, Peers as FlatBuffersPeers, PeersArgs,
    Ping as FlatBuffersPing, PingArgs, Pong as FlatBuffersPong, PongArgs,
    Secp256k1Signature as FlatBuffersSecp256k1Signature, Secp256k1SignatureArgs,
    Signature as FlatBuffersSignature, Transaction as FlatBuffersTransaction, TransactionArgs,
    Verack as FlatBuffersVerack, VerackArgs, Version as FlatBuffersVersion, VersionArgs,
};
use crate::types::{
    Address, Command, GetData, GetPeers, Inv,
    IpAddress::{Ipv4 as WitnetIpv4, Ipv6 as WitnetIpv6},
    Message, Peers, Ping, Pong, Verack, Version,
};

use flatbuffers::FlatBufferBuilder;

const FTB_SIZE: usize = 1024;

#[derive(Debug, Clone, Copy)]
struct GetPeersFlatbufferArgs {
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct GetPeersWitnetArgs {
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct PeersFlatbufferArgs<'a> {
    magic: u16,
    peers: &'a [Address],
}

#[derive(Debug, Clone, Copy)]
struct PeersWitnetArgs<'a> {
    magic: u16,
    peers: FlatBuffersPeers<'a>,
}

#[derive(Debug, Clone, Copy)]
struct PingFlatbufferArgs {
    magic: u16,
    nonce: u64,
}

#[derive(Debug, Clone, Copy)]
struct PingWitnetArgs {
    nonce: u64,
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct PongFlatbufferArgs {
    magic: u16,
    nonce: u64,
}

#[derive(Debug, Clone, Copy)]
struct PongWitnetArgs {
    magic: u16,
    nonce: u64,
}

#[derive(Debug, Clone, Copy)]
struct VerackFlatbufferArgs {
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct VerackWitnetArgs {
    magic: u16,
}

#[derive(Debug, Clone, Copy)]
struct VersionFlatbufferArgs<'a> {
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

#[derive(Debug, Clone)]
struct BlockFlatbufferArgs<'a> {
    magic: u16,
    header: BlockHeaderWithProof,
    txn_count: u32,
    txns: &'a [Transaction],
}

#[derive(Debug, Clone)]
struct VersionWitnetArgs {
    capabilities: u64,
    genesis: u64,
    last_epoch: u32,
    magic: u16,
    nonce: u64,
    receiver_address: Address,
    sender_address: Address,
    timestamp: i64,
    user_agent: String,
    version: u32,
}

#[derive(Debug, Clone, Copy)]
struct InvFlatbufferArgs<'a> {
    magic: u16,
    inventory: &'a [InvVector],
}

#[derive(Debug, Clone, Copy)]
struct InvWitnetArgs<'a> {
    magic: u16,
    inventory: FlatBuffersInv<'a>,
}

#[derive(Debug, Clone, Copy)]
struct GetDataFlatbufferArgs<'a> {
    magic: u16,
    inventory: &'a [InvElem],
}

#[derive(Debug, Clone, Copy)]
struct GetDataWitnetArgs<'a> {
    magic: u16,
    inventory: FlatBuffersGetData<'a>,
}
pub trait TryFrom<T>: Sized {
    type Error;

    fn try_from(value: T) -> Result<Self, Self::Error>;
}

impl TryFrom<Vec<u8>> for Message {
    type Error = &'static str;
    // type Error = Err<&'static str>;
    fn try_from(bytes: Vec<u8>) -> Result<Self, &'static str> {
        // Get Flatbuffers Message
        let message = get_root_as_message(&bytes);

        // Get magic field from message
        let magic = message.magic();

        // Create witnet's message to decode a flatbuffer message
        match message.command_type() {
            FlatBuffersCommand::Ping => message
                .command_as_ping()
                .map(|ping| {
                    create_ping_message(PingWitnetArgs {
                        nonce: ping.nonce(),
                        magic,
                    })
                })
                .ok_or(""),
            FlatBuffersCommand::Pong => message
                .command_as_pong()
                .map(|pong| {
                    create_pong_message(PongWitnetArgs {
                        nonce: pong.nonce(),
                        magic,
                    })
                })
                .ok_or(""),
            FlatBuffersCommand::GetPeers => {
                Ok(create_get_peers_message(GetPeersWitnetArgs { magic }))
            }
            FlatBuffersCommand::Peers => message
                .command_as_peers()
                .and_then(|peers| create_peers_message(PeersWitnetArgs { magic, peers }))
                .ok_or(""),
            FlatBuffersCommand::Verack => Ok(create_verack_message(VerackWitnetArgs { magic })),
            FlatBuffersCommand::Version => message
                .command_as_version()
                .and_then(|command| {
                    // Get ftb addresses and create witnet addresses
                    let sender_address = command.sender_address().and_then(create_address);
                    let receiver_address = command.receiver_address().and_then(create_address);
                    // Check if sender address and receiver address exist
                    if sender_address.and(receiver_address).is_some() {
                        Some(create_version_message(VersionWitnetArgs {
                            version: command.version(),
                            timestamp: command.timestamp(),
                            capabilities: command.capabilities(),
                            sender_address: sender_address.unwrap(),
                            receiver_address: receiver_address.unwrap(),
                            // FIXME(#65): user_agent field should be required as specified in ftb schema
                            user_agent: command.user_agent().to_string(),
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
            FlatBuffersCommand::Block => message
                .command_as_block()
                .map(|block| {
                    // Get Header
                    let header_ftb = block.header();
                    let version = header_ftb.version();
                    // Get CheckpointBeacon
                    let hash: Hash = match header_ftb.beacon().hash_prev_block().type_() {
                        FlatBuffersHashType::SHA256 => {
                            let mut sha256: SHA256 = [0; 32];
                            let sha256_bytes = header_ftb.beacon().hash_prev_block().bytes();
                            sha256.copy_from_slice(sha256_bytes);

                            Hash::SHA256(sha256)
                        }
                    };
                    let beacon = CheckpointBeacon {
                        checkpoint: header_ftb.beacon().checkpoint(),
                        hash_prev_block: hash,
                    };
                    // Get hash merkle root
                    let hash_merkle_root: Hash = match header_ftb.hash_merkle_root().type_() {
                        FlatBuffersHashType::SHA256 => {
                            let mut sha256: SHA256 = [0; 32];
                            let sha256_bytes = header_ftb.hash_merkle_root().bytes();
                            sha256.copy_from_slice(sha256_bytes);

                            Hash::SHA256(sha256)
                        }
                    };
                    // Get proof of leadership
                    let signature = header_ftb
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

                            Some(signature)
                        });
                    let influence = header_ftb.proof().influence();
                    let proof = LeadershipProof {
                        block_sig: Some(Signature::Secp256k1(signature.unwrap())),
                        influence,
                    };
                    // Create BlockHeaderWithProof
                    let header = BlockHeaderWithProof {
                        block_header: BlockHeader {
                            version,
                            beacon,
                            hash_merkle_root,
                        },
                        proof,
                    };
                    // Get transaction count
                    let txn_count = block.txn_count();
                    // Get transactions
                    let len = block.txns().len();
                    let mut counter = 0;
                    let mut _tx_ftb;
                    let mut txns = Vec::new();
                    while counter < len {
                        _tx_ftb = block.txns().get(counter);
                        // Call create_transaction(ftb_tx) in order to get native Transaction
                        txns.push(Transaction);
                        counter += 1;
                    }
                    // Create Message with command
                    Message {
                        kind: Command::Block(Block {
                            header,
                            txn_count,
                            txns,
                        }),
                        magic,
                    }
                })
                .ok_or(""),
            FlatBuffersCommand::Inv => message
                .command_as_inv()
                .and_then(|inv| {
                    Some(create_inv_message(InvWitnetArgs {
                        magic,
                        inventory: inv,
                    }))
                })
                .ok_or(""),
            FlatBuffersCommand::GetData => message
                .command_as_get_data()
                .and_then(|get_data| {
                    Some(create_get_data_message(GetDataWitnetArgs {
                        magic,
                        inventory: get_data,
                    }))
                })
                .ok_or(""),
            FlatBuffersCommand::NONE => Err(""),
        }
    }
}

impl Into<Vec<u8>> for Message {
    fn into(self) -> Vec<u8> {
        let mut builder = flatbuffers::FlatBufferBuilder::new_with_capacity(FTB_SIZE);

        // Create flatbuffer to encode a witnet message
        match self.kind {
            Command::GetPeers(GetPeers) => create_get_peers_flatbuffer(
                &mut builder,
                GetPeersFlatbufferArgs { magic: self.magic },
            ),
            Command::Peers(Peers { peers }) => create_peers_flatbuffer(
                &mut builder,
                PeersFlatbufferArgs {
                    magic: self.magic,
                    peers: &peers,
                },
            ),
            Command::Ping(Ping { nonce }) => create_ping_flatbuffer(
                &mut builder,
                PingFlatbufferArgs {
                    magic: self.magic,
                    nonce,
                },
            ),
            Command::Pong(Pong { nonce }) => create_pong_flatbuffer(
                &mut builder,
                PongFlatbufferArgs {
                    magic: self.magic,
                    nonce,
                },
            ),
            Command::Verack(Verack) => {
                create_verack_flatbuffer(&mut builder, VerackFlatbufferArgs { magic: self.magic })
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
                VersionFlatbufferArgs {
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
                header,
                txn_count,
                txns,
            }) => create_block_flatbuffer(
                &mut builder,
                BlockFlatbufferArgs {
                    magic: self.magic,
                    header,
                    txn_count,
                    txns: &txns,
                },
            ),
            Command::Inv(Inv { inventory }) => create_inv_flatbuffer(
                &mut builder,
                InvFlatbufferArgs {
                    magic: self.magic,
                    inventory: &inventory,
                },
            ),
            Command::GetData(GetData { inventory }) => create_get_data_flatbuffer(
                &mut builder,
                GetDataFlatbufferArgs {
                    magic: self.magic,
                    inventory: &inventory,
                },
            ),
        }
    }
}

// Encode a flatbuffer from a flatbuffers message
fn build_flatbuffer(
    builder: &mut FlatBufferBuilder,
    message: flatbuffers::WIPOffset<FlatBuffersMessage>,
) -> Vec<u8> {
    builder.finish(message, None);
    builder.finished_data().to_vec()
}

// Create witnet ip address
fn create_address(address: FlatBufferAddress) -> Option<Address> {
    match address.ip_type() {
        FlatBuffersIpAddress::Ipv4 => address
            .ip_as_ipv_4()
            .map(|ipv4| create_ipv4_address(ipv4.ip(), address.port())),
        FlatBuffersIpAddress::Ipv6 => match address.ip_as_ipv_6() {
            Some(hextets) => Some(create_ipv6_address(
                hextets.ip0(),
                hextets.ip1(),
                hextets.ip2(),
                hextets.ip3(),
                address.port(),
            )),
            None => None,
        },
        FlatBuffersIpAddress::NONE => None,
    }
}

// Create a get peers flatbuffer to encode a witnet's get peers message
fn create_get_peers_flatbuffer(
    builder: &mut FlatBufferBuilder,
    get_peers_args: GetPeersFlatbufferArgs,
) -> Vec<u8> {
    let get_peers_command = FlatBuffersGetPeers::create(builder, &GetPeersArgs {});

    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: get_peers_args.magic,
            command_type: FlatBuffersCommand::GetPeers,
            command: Some(get_peers_command.as_union_value()),
        },
    );
    build_flatbuffer(builder, message)
}

// Create a witnet's get peers message to decode a flatbuffers' get peers message
fn create_get_peers_message(get_peers_args: GetPeersWitnetArgs) -> Message {
    Message {
        kind: Command::GetPeers(GetPeers),
        magic: get_peers_args.magic,
    }
}

// Create witnet ipv4 address
fn create_ipv4_address(ip: u32, port: u16) -> Address {
    Address {
        ip: WitnetIpv4 { ip },
        port,
    }
}

// Create witnet ipv6 address
fn create_ipv6_address(ip0: u32, ip1: u32, ip2: u32, ip3: u32, port: u16) -> Address {
    Address {
        ip: WitnetIpv6 { ip0, ip1, ip2, ip3 },
        port,
    }
}

// Create a peers flatbuffer to encode a witnet's peers message
fn create_peers_flatbuffer(
    builder: &mut FlatBufferBuilder,
    peers_args: PeersFlatbufferArgs,
) -> Vec<u8> {
    let addresses_command: Vec<flatbuffers::WIPOffset<FlatBufferAddress>> = peers_args
        .peers
        .iter()
        .map(|peer: &Address| match peer.ip {
            WitnetIpv4 { ip } => {
                let ip_command = FlatBuffersIpv4::create(builder, &FlatBuffersIpv4Args { ip });
                FlatBufferAddress::create(
                    builder,
                    &AddressArgs {
                        ip_type: FlatBuffersIpAddress::Ipv4,
                        ip: Some(ip_command.as_union_value()),
                        port: peer.port,
                    },
                )
            }
            WitnetIpv6 { ip0, ip1, ip2, ip3 } => {
                let ip_command =
                    FlatBuffersIpv6::create(builder, &FlatBuffersIpv6Args { ip0, ip1, ip2, ip3 });
                FlatBufferAddress::create(
                    builder,
                    &AddressArgs {
                        ip_type: FlatBuffersIpAddress::Ipv6,
                        ip: Some(ip_command.as_union_value()),
                        port: peer.port,
                    },
                )
            }
        })
        .collect();

    let addresses = Some(builder.create_vector(&addresses_command));
    let peers_command = FlatBuffersPeers::create(builder, &PeersArgs { peers: addresses });

    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: peers_args.magic,
            command_type: FlatBuffersCommand::Peers,
            command: Some(peers_command.as_union_value()),
        },
    );
    build_flatbuffer(builder, message)
}

// Create a witnet's peers message to decode a flatbuffers' peers message
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

// Create a ping flatbuffer to encode a witnet's ping message
fn create_ping_flatbuffer(
    builder: &mut FlatBufferBuilder,
    ping_args: PingFlatbufferArgs,
) -> Vec<u8> {
    let ping_command = FlatBuffersPing::create(
        builder,
        &PingArgs {
            nonce: ping_args.nonce.to_owned(),
        },
    );
    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: ping_args.magic,
            command_type: FlatBuffersCommand::Ping,
            command: Some(ping_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create a witnet's ping message to decode a flatbuffers' ping message
fn create_ping_message(ping_args: PingWitnetArgs) -> Message {
    Message {
        kind: Command::Ping(Ping {
            nonce: ping_args.nonce,
        }),
        magic: ping_args.magic,
    }
}

// Create a pong flatbuffer to encode a witnet's pong message
fn create_pong_flatbuffer(
    builder: &mut FlatBufferBuilder,
    pong_args: PongFlatbufferArgs,
) -> Vec<u8> {
    let pong_command = FlatBuffersPong::create(
        builder,
        &PongArgs {
            nonce: pong_args.nonce,
        },
    );
    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: pong_args.magic,
            command_type: FlatBuffersCommand::Pong,
            command: Some(pong_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create a witnet's pong message to decode a flatbuffers' pong message
fn create_pong_message(pong_args: PongWitnetArgs) -> Message {
    Message {
        kind: Command::Pong(Pong {
            nonce: pong_args.nonce,
        }),
        magic: pong_args.magic,
    }
}

// Create a verack flatbuffer to encode a witnet's verack message
fn create_verack_flatbuffer(
    builder: &mut FlatBufferBuilder,
    verack_args: VerackFlatbufferArgs,
) -> Vec<u8> {
    let verack_command = FlatBuffersVerack::create(builder, &VerackArgs {});

    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: verack_args.magic,
            command_type: FlatBuffersCommand::Verack,
            command: Some(verack_command.as_union_value()),
        },
    );
    build_flatbuffer(builder, message)
}

// Create a witnet's verack message to decode a flatbuffers' verack message
fn create_verack_message(verack_args: VerackWitnetArgs) -> Message {
    Message {
        kind: Command::Verack(Verack),
        magic: verack_args.magic,
    }
}

// Create a version flatbuffer to encode a witnet's version message
fn create_version_flatbuffer(
    builder: &mut FlatBufferBuilder,
    version_args: VersionFlatbufferArgs,
) -> Vec<u8> {
    let sender_address_command = match version_args.sender_address.ip {
        WitnetIpv4 { ip } => {
            let ip_command = FlatBuffersIpv4::create(builder, &FlatBuffersIpv4Args { ip });
            FlatBufferAddress::create(
                builder,
                &AddressArgs {
                    ip_type: FlatBuffersIpAddress::Ipv4,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.sender_address.port,
                },
            )
        }
        WitnetIpv6 { ip0, ip1, ip2, ip3 } => {
            let ip_command =
                FlatBuffersIpv6::create(builder, &FlatBuffersIpv6Args { ip0, ip1, ip2, ip3 });
            FlatBufferAddress::create(
                builder,
                &AddressArgs {
                    ip_type: FlatBuffersIpAddress::Ipv6,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.sender_address.port,
                },
            )
        }
    };

    let receiver_address_command = match version_args.receiver_address.ip {
        WitnetIpv4 { ip } => {
            let ip_command = FlatBuffersIpv4::create(builder, &FlatBuffersIpv4Args { ip });
            FlatBufferAddress::create(
                builder,
                &AddressArgs {
                    ip_type: FlatBuffersIpAddress::Ipv4,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.receiver_address.port,
                },
            )
        }
        WitnetIpv6 { ip0, ip1, ip2, ip3 } => {
            let ip_command =
                FlatBuffersIpv6::create(builder, &FlatBuffersIpv6Args { ip0, ip1, ip2, ip3 });
            FlatBufferAddress::create(
                builder,
                &AddressArgs {
                    ip_type: FlatBuffersIpAddress::Ipv6,
                    ip: Some(ip_command.as_union_value()),
                    port: version_args.receiver_address.port,
                },
            )
        }
    };

    let user_agent = Some(builder.create_string(&version_args.user_agent));
    let version_command = FlatBuffersVersion::create(
        builder,
        &VersionArgs {
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

    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: version_args.magic,
            command_type: FlatBuffersCommand::Version,
            command: Some(version_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create a witnet's version message to decode a flatbuffers' version message
fn create_version_message(version_args: VersionWitnetArgs) -> Message {
    Message {
        kind: Command::Version(Version {
            version: version_args.version,
            timestamp: version_args.timestamp,
            capabilities: version_args.capabilities,
            sender_address: version_args.sender_address,
            receiver_address: version_args.receiver_address,
            user_agent: version_args.user_agent,
            last_epoch: version_args.last_epoch,
            genesis: version_args.genesis,
            nonce: version_args.nonce,
        }),
        magic: version_args.magic,
    }
}

// Create a block flatbuffer to encode a witnet's version message
fn create_block_flatbuffer(
    builder: &mut FlatBufferBuilder,
    block_args: BlockFlatbufferArgs,
) -> Vec<u8> {
    // Create checkpoint beacon flatbuffer
    let hash_prev_block_args = match block_args.header.block_header.beacon.hash_prev_block {
        Hash::SHA256(hash) => HashArgs {
            type_: FlatBuffersHashType::SHA256,
            bytes: Some(builder.create_vector(&hash)),
        },
    };
    let hash_prev_block = Some(FlatBuffersHash::create(builder, &hash_prev_block_args));
    let beacon = Some(FlatBuffersCheckpointBeacon::create(
        builder,
        &FlatBuffersCheckpointBeaconArgs {
            checkpoint: block_args.header.block_header.beacon.checkpoint,
            hash_prev_block,
        },
    ));
    // Create hash merkle root flatbuffer
    let hash_merkle_root_args = match block_args.header.block_header.hash_merkle_root {
        Hash::SHA256(hash) => HashArgs {
            type_: FlatBuffersHashType::SHA256,
            bytes: Some(builder.create_vector(&hash)),
        },
    };
    let hash_merkle_root = Some(FlatBuffersHash::create(builder, &hash_merkle_root_args));
    // Create proof of leadership flatbuffer
    let block_sig_type =
        block_args
            .header
            .proof
            .block_sig
            .clone()
            .map(|signature| match signature {
                Signature::Secp256k1(_) => FlatBuffersSignature::Secp256k1Signature,
            });
    let block_sig = block_args
        .header
        .proof
        .block_sig
        .map(|signature| match signature {
            Signature::Secp256k1(secp256k1) => {
                let mut s = secp256k1.s.to_vec();
                s.push(secp256k1.v);
                let r_ftb = Some(builder.create_vector(&secp256k1.r));
                let s_ftb = Some(builder.create_vector(&s));

                FlatBuffersSecp256k1Signature::create(
                    builder,
                    &Secp256k1SignatureArgs { r: r_ftb, s: s_ftb },
                )
                .as_union_value()
            }
        });
    let proof = Some(FlatBuffersLeadershipProof::create(
        builder,
        &LeadershipProofArgs {
            block_sig_type: block_sig_type.unwrap_or(FlatBuffersSignature::NONE),
            block_sig,
            influence: block_args.header.proof.influence,
        },
    ));
    // Create block header flatbuffer
    let header = Some(FlatBuffersBlockHeader::create(
        builder,
        &BlockHeaderArgs {
            version: block_args.header.block_header.version,
            beacon,
            hash_merkle_root,
            proof,
        },
    ));
    // Create transaction array flatbuffer
    let txns: Vec<flatbuffers::WIPOffset<FlatBuffersTransaction>> = block_args
        .txns
        .iter()
        .map(|_tx: &Transaction| FlatBuffersTransaction::create(builder, &TransactionArgs {}))
        .collect();
    let txns_ftb = Some(builder.create_vector(&txns));
    // Create block command flatbuffer
    let block_command = FlatBuffersBlock::create(
        builder,
        &BlockArgs {
            header,
            txn_count: block_args.txn_count,
            txns: txns_ftb,
        },
    );
    // Create message flatbuffer
    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: block_args.magic,
            command_type: FlatBuffersCommand::Block,
            command: Some(block_command.as_union_value()),
        },
    );

    build_flatbuffer(builder, message)
}

// Create an inv flatbuffer to encode a witnet's inv message
fn create_inv_flatbuffer(builder: &mut FlatBufferBuilder, inv_args: InvFlatbufferArgs) -> Vec<u8> {
    // Create vector of flatbuffers' inv vectors
    let ftb_inv_vectors: Vec<flatbuffers::WIPOffset<FlatBuffersInvVector>> = inv_args
        .inventory
        .iter()
        .map(|inv_vector: &InvVector| {
            // Create flatbuffers' hash bytes
            let hash = match inv_vector {
                InvVector::Error(hash) => hash,
                InvVector::Tx(hash) => hash,
                InvVector::Block(hash) => hash,
                InvVector::DataRequest(hash) => hash,
                InvVector::DataResult(hash) => hash,
            };

            // Get hash bytes
            let bytes = match hash {
                Hash::SHA256(bytes) => builder.create_vector(bytes),
            };

            // Create flatbuffers' hash
            let ftb_hash = match hash {
                Hash::SHA256(_) => FlatBuffersHash::create(
                    builder,
                    &HashArgs {
                        type_: FlatBuffersHashType::SHA256,
                        bytes: Some(bytes),
                    },
                ),
            };

            // Create flatbuffers inv vector type
            let ftb_type = match inv_vector {
                InvVector::Error(_) => FlatBuffersInvVectorType::Error,
                InvVector::Tx(_) => FlatBuffersInvVectorType::Tx,
                InvVector::Block(_) => FlatBuffersInvVectorType::Block,
                InvVector::DataRequest(_) => FlatBuffersInvVectorType::DataRequest,
                InvVector::DataResult(_) => FlatBuffersInvVectorType::DataResult,
            };

            // Create flatbuffers inv vector
            FlatBuffersInvVector::create(
                builder,
                &InvVectorArgs {
                    type_: ftb_type,
                    hash: Some(ftb_hash),
                },
            )
        })
        .collect();

    // Create flatbuffers' vector of flatbuffers' inv vectors
    let ftb_inv_vectors = Some(builder.create_vector(&ftb_inv_vectors));

    // Create inv flatbuffers command
    let inv_command = FlatBuffersInv::create(
        builder,
        &InvArgs {
            inventory: ftb_inv_vectors,
        },
    );

    // Create flatbuffers message
    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: inv_args.magic,
            command_type: FlatBuffersCommand::Inv,
            command: Some(inv_command.as_union_value()),
        },
    );

    // Get vector of bytes from flatbuffer message
    build_flatbuffer(builder, message)
}

// Create an get_data flatbuffer to encode a witnet's get_data message
fn create_get_data_flatbuffer(
    builder: &mut FlatBufferBuilder,
    get_data_args: GetDataFlatbufferArgs,
) -> Vec<u8> {
    // Create vector of flatbuffers' inv elements
    let ftb_inv_elems: Vec<flatbuffers::WIPOffset<FlatBuffersInvElem>> = get_data_args
        .inventory
        .iter()
        .map(|inv_elem: &InvElem| {
            // Create flatbuffers' hash bytes
            let hash = match inv_elem {
                InvElem::Error(hash) => hash,
                InvElem::Tx(hash) => hash,
                InvElem::Block(hash) => hash,
                InvElem::DataRequest(hash) => hash,
                InvElem::DataResult(hash) => hash,
            };

            // Get hash bytes
            let bytes = match hash {
                Hash::SHA256(bytes) => builder.create_vector(bytes),
            };

            // Create flatbuffers' hash
            let ftb_hash = match hash {
                Hash::SHA256(_) => FlatBuffersHash::create(
                    builder,
                    &HashArgs {
                        type_: FlatBuffersHashType::SHA256,
                        bytes: Some(bytes),
                    },
                ),
            };

            // Create flatbuffers inv elem type
            let ftb_type = match inv_elem {
                InvElem::Error(_) => FlatBuffersInvElemType::Error,
                InvElem::Tx(_) => FlatBuffersInvElemType::Tx,
                InvElem::Block(_) => FlatBuffersInvElemType::Block,
                InvElem::DataRequest(_) => FlatBuffersInvElemType::DataRequest,
                InvElem::DataResult(_) => FlatBuffersInvElemType::DataResult,
            };

            // Create flatbuffers inv elem
            FlatBuffersInvElem::create(
                builder,
                &InvElemArgs {
                    type_: ftb_type,
                    hash: Some(ftb_hash),
                },
            )
        })
        .collect();

    // Create flatbuffers' vector of flatbuffers' inv elements
    let ftb_inv_elems = Some(builder.create_vector(&ftb_inv_elems));

    // Create get_data flatbuffers command
    let get_data_command = FlatBuffersGetData::create(
        builder,
        &GetDataArgs {
            inventory: ftb_inv_elems,
        },
    );

    // Create flatbuffers message
    let message = FlatBuffersMessage::create(
        builder,
        &MessageArgs {
            magic: get_data_args.magic,
            command_type: FlatBuffersCommand::GetData,
            command: Some(get_data_command.as_union_value()),
        },
    );

    // Get vector of bytes from flatbuffer message
    build_flatbuffer(builder, message)
}

// Create a witnet's inv message to decode a flatbuffers' inv message
fn create_inv_message(inv_args: InvWitnetArgs) -> Message {
    // Get inventory vectors (flatbuffers' types)
    let ftb_inv_vectors = inv_args.inventory.inventory();
    let len = ftb_inv_vectors.len();

    // Create empty vector of inventory vectors
    let mut inv_vectors = Vec::new();

    // Create all inventory vectors (witnet's types) and add them to a vector
    for i in 0..len {
        let inv_vector = create_inv_vector(ftb_inv_vectors.get(i));
        inv_vectors.push(inv_vector);
    }

    // Create message
    Message {
        magic: inv_args.magic,
        kind: Command::Inv(Inv {
            inventory: inv_vectors,
        }),
    }
}

// Create a witnet's inv vector from a flatbuffers' inv vector
fn create_inv_vector(inv_vector: FlatBuffersInvVector) -> InvVector {
    // Create inventory vector hash
    let hash = create_hash(inv_vector.hash());

    // Create inventory vector
    match inv_vector.type_() {
        FlatBuffersInvVectorType::Error => InvVector::Error(hash),
        FlatBuffersInvVectorType::Tx => InvVector::Tx(hash),
        FlatBuffersInvVectorType::Block => InvVector::Block(hash),
        FlatBuffersInvVectorType::DataRequest => InvVector::DataRequest(hash),
        FlatBuffersInvVectorType::DataResult => InvVector::DataResult(hash),
    }
}

// Create a witnet's get_data message to decode a flatbuffers' get_data message
fn create_get_data_message(get_data_args: GetDataWitnetArgs) -> Message {
    // Get inventory elements (flatbuffers' types)
    let ftb_inv_elems = get_data_args.inventory.inventory();
    let len = ftb_inv_elems.len();

    // Create empty vector of inventory elements
    let mut inv_elems = Vec::new();

    // Create all inventory elements (witnet's types) and add them to a vector
    for i in 0..len {
        let inv_elem = create_inv_elem(ftb_inv_elems.get(i));
        inv_elems.push(inv_elem);
    }

    // Create message
    Message {
        magic: get_data_args.magic,
        kind: Command::GetData(GetData {
            inventory: inv_elems,
        }),
    }
}

// Create a witnet's hash from a flatbuffers' hash
fn create_hash(hash: FlatBuffersHash) -> Hash {
    // Get hash bytes
    let mut hash_bytes: SHA256 = [0; 32];
    hash_bytes.copy_from_slice(hash.bytes());

    // Build hash
    match hash.type_() {
        FlatBuffersHashType::SHA256 => Hash::SHA256(hash_bytes),
    }
}
