extern crate flatbuffers;

use crate::chain::{
    Block, BlockHeader, CheckpointBeacon, CommitInput, CommitOutput, DataRequestInput,
    DataRequestOutput, Hash, Input, InventoryEntry, KeyedSignature, LeadershipProof, Output,
    PublicKeyHash, RevealInput, RevealOutput, Secp256k1Signature, Signature, TallyOutput,
    Transaction, ValueTransferInput, ValueTransferOutput, SHA256,
};
use crate::flatbuffers::protocol_generated::protocol;

use crate::types::{
    Address, Command, GetPeers, InventoryAnnouncement, InventoryRequest,
    IpAddress::{Ipv4, Ipv6},
    LastBeacon, Message, Peers, Ping, Pong, Verack, Version,
};

pub const FTB_SIZE: usize = 1024;

type FlatbufferInputVector<'a> =
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::Input<'a>>>;
type FlatbufferKeyedSignatureVector<'a> =
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::KeyedSignature<'a>>>;
type FlatbufferOutputVector<'a> =
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::Output<'a>>>;

////////////////////////////////////////////////////////
// ARGS
////////////////////////////////////////////////////////

// COMMAND ARGS
#[derive(Debug, Clone, Copy)]
struct EmptyCommandArgs {
    magic: u16,
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

////////////////////////////////////////////////////////
// INTO TRAIT (Message ----> Vec<u8>)
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

        // Build Witnet's message to decode a flatbuffer message
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

            protocol::Command::Transaction => panic!("Unimplemented"), // Unimplemented,
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
                    let mut tx_ftb;
                    let mut block_txns = Vec::new();
                    while counter < len {
                        tx_ftb = block.txns().get(counter);
                        block_txns.push(create_transaction(tx_ftb));
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
                    // Build BlockHeader
                    let header = BlockHeader {
                        version,
                        beacon,
                        hash_merkle_root,
                    };

                    // Build Message with command
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
// FROM TRAIT AUX FUNCTIONS: to create Witnet's types
////////////////////////////////////////////////////////

fn create_transaction(ftb_tx: protocol::Transaction) -> Transaction {
    let ftb_inputs = ftb_tx.inputs();
    let ftb_outputs = ftb_tx.outputs();
    let ftb_keyed_signatures = ftb_tx.signatures();

    Transaction {
        inputs: create_input_vector(&ftb_inputs),
        outputs: create_output_vector(&ftb_outputs),
        signatures: create_keyed_signature_vector(&ftb_keyed_signatures),
        version: ftb_tx.version(),
    }
}

fn create_input_vector(ftb_inputs: &FlatbufferInputVector) -> Vec<Input> {
    let mut counter = 0;
    let mut inputs = vec![];
    while counter < ftb_inputs.len() {
        let input = create_input(ftb_inputs.get(counter));
        inputs.push(input);
        counter += 1;
    }

    inputs
}

fn create_input(ftb_input: protocol::Input) -> Input {
    match ftb_input.input_type() {
        protocol::InputUnion::DataRequestInput => ftb_input
            .input_as_data_request_input()
            .map(|data_request_input| {
                Input::DataRequest(DataRequestInput {
                    transaction_id: {
                        let mut transaction_id = [0; 32];
                        transaction_id.copy_from_slice(&data_request_input.transaction_id()[0..32]);

                        Hash::SHA256(transaction_id)
                    },
                    output_index: data_request_input.output_index(),
                    poe: {
                        let mut poe = [0; 32];
                        poe.copy_from_slice(&data_request_input.poe()[0..32]);

                        poe
                    },
                })
            })
            .unwrap(),
        protocol::InputUnion::CommitInput => ftb_input
            .input_as_commit_input()
            .map(|commit_input| {
                Input::Commit(CommitInput {
                    transaction_id: {
                        let mut transaction_id = [0; 32];
                        transaction_id.copy_from_slice(&commit_input.transaction_id()[0..32]);

                        Hash::SHA256(transaction_id)
                    },
                    output_index: commit_input.output_index(),
                    reveal: {
                        let mut reveal = [0; 32];
                        reveal.copy_from_slice(&commit_input.reveal()[0..32]);

                        reveal.to_vec()
                    },
                    nonce: commit_input.nonce(),
                })
            })
            .unwrap(),
        protocol::InputUnion::RevealInput => ftb_input
            .input_as_reveal_input()
            .map(|reveal_input| {
                Input::Reveal(RevealInput {
                    transaction_id: {
                        let mut transaction_id = [0; 32];
                        transaction_id.copy_from_slice(&reveal_input.transaction_id()[0..32]);

                        Hash::SHA256(transaction_id)
                    },
                    output_index: reveal_input.output_index(),
                })
            })
            .unwrap(),
        protocol::InputUnion::ValueTransferInput => ftb_input
            .input_as_value_transfer_input()
            .map(|value_transfer_input| {
                Input::ValueTransfer(ValueTransferInput {
                    transaction_id: {
                        let mut transaction_id = [0; 32];
                        transaction_id
                            .copy_from_slice(&value_transfer_input.transaction_id()[0..32]);

                        Hash::SHA256(transaction_id)
                    },
                    output_index: value_transfer_input.output_index(),
                })
            })
            .unwrap(),
        _ => unreachable!(), // All Input types are covered
    }
}

fn create_output_vector(ftb_outputs: &FlatbufferOutputVector) -> Vec<Output> {
    let mut counter = 0;
    let mut outputs = vec![];
    while counter < ftb_outputs.len() {
        let output = create_output(ftb_outputs.get(counter));
        outputs.push(output);
        counter += 1;
    }

    outputs
}

fn create_output(ftb_output: protocol::Output) -> Output {
    match ftb_output.output_type() {
        protocol::OutputUnion::ValueTransferOutput => ftb_output
            .output_as_value_transfer_output()
            .map(|value_transfer_output| {
                Output::ValueTransfer(ValueTransferOutput {
                    pkh: create_pkh(value_transfer_output.pkh()),
                    value: value_transfer_output.value(),
                })
            })
            .unwrap(),

        protocol::OutputUnion::DataRequestOutput => ftb_output
            .output_as_data_request_output()
            .map(|data_request_output| {
                Output::DataRequest(DataRequestOutput {
                    backup_witnesses: data_request_output.backup_witnesses(),
                    commit_fee: data_request_output.commit_fee(),
                    pkh: create_pkh(data_request_output.pkh()),
                    reveal_fee: data_request_output.reveal_fee(),
                    data_request: {
                        let mut arr = [0; 32];
                        arr.copy_from_slice(data_request_output.data_request());

                        arr.to_vec()
                    },
                    tally_fee: data_request_output.tally_fee(),
                    time_lock: data_request_output.time_lock(),
                    value: data_request_output.value(),
                    witnesses: data_request_output.witnesses(),
                })
            })
            .unwrap(),

        protocol::OutputUnion::CommitOutput => ftb_output
            .output_as_commit_output()
            .map(|commit_output| {
                Output::Commit(CommitOutput {
                    commitment: create_hash(commit_output.commitment()),
                    value: commit_output.value(),
                })
            })
            .unwrap(),

        protocol::OutputUnion::RevealOutput => ftb_output
            .output_as_reveal_output()
            .map(|reveal_output| {
                Output::Reveal(RevealOutput {
                    pkh: create_pkh(reveal_output.pkh()),
                    reveal: {
                        let mut reveal = [0; 32];
                        reveal.copy_from_slice(&reveal_output.reveal()[0..32]);

                        reveal.to_vec()
                    },
                    value: reveal_output.value(),
                })
            })
            .unwrap(),

        protocol::OutputUnion::ConsensusOutput => ftb_output
            .output_as_consensus_output()
            .map(|consensus_output| {
                Output::Tally(TallyOutput {
                    pkh: create_pkh(consensus_output.pkh()),
                    result: {
                        let mut result = [0; 32];
                        result.copy_from_slice(&consensus_output.result()[0..32]);

                        result.to_vec()
                    },
                    value: consensus_output.value(),
                })
            })
            .unwrap(),
        _ => unreachable!(), // All output types are covered
    }
}

fn create_keyed_signature_vector(
    ftb_keyed_signatures: &FlatbufferKeyedSignatureVector,
) -> Vec<KeyedSignature> {
    let mut counter = 0;
    let mut keyed_signatures = vec![];
    while counter < ftb_keyed_signatures.len() {
        let keyed_signature = create_keyed_signature(ftb_keyed_signatures.get(counter));
        keyed_signatures.push(keyed_signature);
        counter += 1;
    }

    keyed_signatures
}

fn create_keyed_signature(ftb_keyed_signature: protocol::KeyedSignature) -> KeyedSignature {
    let signature = match ftb_keyed_signature.signature_type() {
        protocol::Signature::Secp256k1Signature => ftb_keyed_signature
            .signature_as_secp_256k_1signature()
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
            })
            .unwrap(),
        _ => unreachable!(), // All keyed signatures types covered
    };

    KeyedSignature {
        public_key: {
            let mut public_key = [0; 32];
            public_key.copy_from_slice(&ftb_keyed_signature.public_key()[0..32]);
            public_key
        },
        signature,
    }
}

// Build a Witnet Ping message to decode a flatbuffers' Ping message
fn create_ping_message(ping_args: HeartbeatCommandsArgs) -> Message {
    Message {
        kind: Command::Ping(Ping {
            nonce: ping_args.nonce,
        }),
        magic: ping_args.magic,
    }
}

// Build a Witnet Pong message to decode a flatbuffers' Pong message
fn create_pong_message(pong_args: HeartbeatCommandsArgs) -> Message {
    Message {
        kind: Command::Pong(Pong {
            nonce: pong_args.nonce,
        }),
        magic: pong_args.magic,
    }
}

// Build a Witnet GetPeers message to decode a flatbuffers' GetPeers message
fn create_get_peers_message(get_peers_args: EmptyCommandArgs) -> Message {
    Message {
        kind: Command::GetPeers(GetPeers),
        magic: get_peers_args.magic,
    }
}

// Build a Witnet's Peers message to decode a flatbuffers' Peers message
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

// Build a Witnet Verack message to decode a flatbuffers' Verack message
fn create_verack_message(verack_args: EmptyCommandArgs) -> Message {
    Message {
        kind: Command::Verack(Verack),
        magic: verack_args.magic,
    }
}

// Build a Witnet Version message to decode a flatbuffers' Version message
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

// Build a Witnet's InventoryAnnouncement message to decode a flatbuffers' InventoryAnnouncement
// message
fn create_inventory_announcement_message(inv_args: InventoryAnnouncementWitnetArgs) -> Message {
    // Get inventory entries (flatbuffers' types)
    let ftb_inv_items = inv_args.inventory.inventory();
    let len = ftb_inv_items.len();

    // Build empty vector of inventory entries
    let mut inv_items = Vec::new();

    // Build all inventory entries (Witnet's types) and add them to a vector
    for i in 0..len {
        let inv_item = create_inventory_entry(ftb_inv_items.get(i));
        inv_items.push(inv_item);
    }

    // Build message
    Message {
        magic: inv_args.magic,
        kind: Command::InventoryAnnouncement(InventoryAnnouncement {
            inventory: inv_items,
        }),
    }
}

// Build a Witnet's InventoryRequest message to decode a flatbuffers' InventoryRequest message
fn create_inventory_request_message(get_data_args: InventoryRequestWitnetArgs) -> Message {
    // Get inventory entries (flatbuffers' types)
    let ftb_inv_items = get_data_args.inventory.inventory();
    let len = ftb_inv_items.len();

    // Build empty vector of inventory entries
    let mut inv_items = Vec::new();

    // Build all inventory entries (Witnet's types) and add them to a vector
    for i in 0..len {
        let inv_item = create_inventory_entry(ftb_inv_items.get(i));
        inv_items.push(inv_item);
    }

    // Build message
    Message {
        magic: get_data_args.magic,
        kind: Command::InventoryRequest(InventoryRequest {
            inventory: inv_items,
        }),
    }
}

// Build a Witnet LastBeacon message to decode flatbuffers' LastBeacon message
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

// Build a Witnet's InventoryEntry from a flatbuffers' InventoryEntry
fn create_inventory_entry(inv_item: protocol::InventoryEntry) -> InventoryEntry {
    // Build inventory entry hash
    let hash = create_hash(inv_item.hash());

    // Build inventory entry
    match inv_item.type_() {
        protocol::InventoryItemType::Error => InventoryEntry::Error(hash),
        protocol::InventoryItemType::Tx => InventoryEntry::Tx(hash),
        protocol::InventoryItemType::Block => InventoryEntry::Block(hash),
        protocol::InventoryItemType::DataRequest => InventoryEntry::DataRequest(hash),
        protocol::InventoryItemType::DataResult => InventoryEntry::DataResult(hash),
    }
}

// Build a Witnet's Hash from a flatbuffers' Hash
fn create_hash(hash: protocol::Hash) -> Hash {
    // Get hash bytes
    let mut hash_bytes: SHA256 = [0; 32];
    hash_bytes.copy_from_slice(hash.bytes());

    // Build hash
    match hash.type_() {
        protocol::HashType::SHA256 => Hash::SHA256(hash_bytes),
    }
}

// Build a Witnet's Public Key Hash from a flatbuffers' array of bytes
fn create_pkh(pkh_bytes: &[u8]) -> PublicKeyHash {
    // Get pkh bytes
    let mut pkh: PublicKeyHash = [0; 20];
    pkh.copy_from_slice(pkh_bytes);

    pkh
}

// Build Witnet IP address
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

// Build Witnet IPv4 address
fn create_ipv4_address(ip: u32, port: u16) -> Address {
    Address {
        ip: Ipv4 { ip },
        port,
    }
}

// Build Witnet IPv6 address
fn create_ipv6_address(ip0: u32, ip1: u32, ip2: u32, ip3: u32, port: u16) -> Address {
    Address {
        ip: Ipv6 { ip0, ip1, ip2, ip3 },
        port,
    }
}
