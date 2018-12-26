extern crate flatbuffers;

use std::convert::Into;

use crate::chain::{
    Block, BlockHeader, CheckpointBeacon, CommitInput, CommitOutput, ConsensusOutput,
    DataRequestOutput, Epoch, Hash, Input, InventoryEntry, KeyedSignature, LeadershipProof, Output,
    RevealInput, RevealOutput, Secp256k1Signature, Signature, TallyInput, Transaction,
    ValueTransferInput, ValueTransferOutput, SHA256,
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
type WIPOffsetOutputVector<'a> = flatbuffers::WIPOffset<
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::Output<'a>>>,
>;
type WOIPOffsetKeyedSignatureVector<'a> = flatbuffers::WIPOffset<
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::KeyedSignature<'a>>>,
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
type WIPOffsetPeersMessage<'a> = flatbuffers::WIPOffset<protocol::Peers<'a>>;
type WIPOffsetPing<'a> = flatbuffers::WIPOffset<protocol::Ping<'a>>;
type WIPOffsetPong<'a> = flatbuffers::WIPOffset<protocol::Pong<'a>>;
type WIPOffsetTransaction<'a> = flatbuffers::WIPOffset<protocol::Transaction<'a>>;
type WIPOffsetTransactionVector<'a> = Option<
    flatbuffers::WIPOffset<
        flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::Transaction<'a>>>,
    >,
>;
type WIPOffsetUnion = std::option::Option<flatbuffers::WIPOffset<flatbuffers::UnionWIPOffset>>;
type WIPOffsetVerackMessage<'a> = flatbuffers::WIPOffset<protocol::Verack<'a>>;
type WIPOffsetVersionMessage<'a> = flatbuffers::WIPOffset<protocol::Version<'a>>;

////////////////////////////////////////////////////////
// ARGS
////////////////////////////////////////////////////////
#[derive(Debug, Clone)]
pub struct BlockArgs {
    pub block_header: BlockHeader,
    pub proof: LeadershipProof,
    pub txns: Vec<Transaction>,
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

            // Inventory
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

type FlatbufferInputVector<'a> =
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::Input<'a>>>;
type FlatbufferOutputVector<'a> =
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::Output<'a>>>;
type FlatbufferKeyedSignatureVector<'a> =
    flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<protocol::KeyedSignature<'a>>>;

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
        protocol::InputUnion::ValueTransferInput => ftb_input
            .input_as_value_transfer_input()
            .map(|value_transfer_input| {
                Input::ValueTransfer(ValueTransferInput {
                    output_index: value_transfer_input.output_index(),
                    transaction_id: {
                        let mut transaction_id = [0; 32];
                        transaction_id
                            .copy_from_slice(&value_transfer_input.transaction_id()[0..32]);
                        transaction_id
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
                        transaction_id
                    },
                    output_index: commit_input.output_index(),
                    poe: {
                        let mut poe = [0; 32];
                        poe.copy_from_slice(&commit_input.poe()[0..32]);
                        poe
                    },
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
                        transaction_id
                    },
                    output_index: reveal_input.output_index(),
                    reveal: {
                        let mut reveal = [0; 32];
                        reveal.copy_from_slice(&reveal_input.reveal()[0..32]);
                        reveal
                    },
                    nonce: reveal_input.nonce(),
                })
            })
            .unwrap(),
        protocol::InputUnion::TallyInput => ftb_input
            .input_as_tally_input()
            .map(|tally_input| {
                Input::Tally(TallyInput {
                    output_index: tally_input.output_index(),
                    transaction_id: {
                        let mut transaction_id = [0; 32];
                        transaction_id.copy_from_slice(&tally_input.transaction_id()[0..32]);
                        transaction_id
                    },
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
                    pkh: create_hash(value_transfer_output.pkh()),
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
                    reveal_fee: data_request_output.reveal_fee(),
                    data_request: {
                        let mut arr = [0; 32];
                        arr.copy_from_slice(data_request_output.data_request());
                        arr
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
                    pkh: create_hash(reveal_output.pkh()),
                    reveal: {
                        let mut reveal = [0; 32];
                        reveal.copy_from_slice(&reveal_output.reveal()[0..32]);
                        reveal
                    },
                    value: reveal_output.value(),
                })
            })
            .unwrap(),

        protocol::OutputUnion::ConsensusOutput => ftb_output
            .output_as_consensus_output()
            .map(|consensus_output| {
                Output::Consensus(ConsensusOutput {
                    pkh: create_hash(consensus_output.pkh()),
                    result: {
                        let mut result = [0; 32];
                        result.copy_from_slice(&consensus_output.result()[0..32]);
                        result
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

////////////////////////////////////////////////////////
// INTO TRAIT AUX FUNCTIONS: to create ftb types
////////////////////////////////////////////////////////

/////////////////////
// FBT BUILDERS
/////////////////////

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
