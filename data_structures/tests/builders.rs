use std::net::SocketAddr;

use witnet_data_structures::builders::*;
use witnet_data_structures::{chain::*, types::*};

#[test]
fn builders_build_last_beacon() {
    let highest_block_checkpoint = CheckpointBeacon {
        checkpoint: 0,
        hash_prev_block: Hash::SHA256([0; 32]),
    };
    let msg = Message {
        kind: Command::LastBeacon(LastBeacon {
            highest_block_checkpoint,
        }),
        magic: MAGIC,
    };
    assert_eq!(msg, Message::build_last_beacon(highest_block_checkpoint));
}

#[test]
fn builders_build_block() {
    // Prepare block header
    let header = BlockHeader {
        version: 0x0000_0001,
        beacon: CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: Hash::SHA256([0; 32]),
        },
        hash_merkle_root: Hash::SHA256([0; 32]),
    };
    let signature = Signature::Secp256k1(Secp256k1Signature {
        r: [0; 32],
        s: [0; 32],
        v: 0,
    });
    let block_header = BlockHeader {
        version: header.version,
        beacon: header.beacon,
        hash_merkle_root: header.hash_merkle_root,
    };
    let proof = LeadershipProof {
        block_sig: Some(signature.clone()),
        influence: 0,
    };
    let keyed_signatures = vec![KeyedSignature {
        public_key: [0; 32],
        signature,
    }];
    let value_transfer_input = Input::ValueTransfer(ValueTransferInput {
        output_index: 0,
        transaction_id: [0; 32],
    });
    let reveal_input = Input::Reveal(RevealInput {
        nonce: 0,
        output_index: 0,
        reveal: [0; 32],
        transaction_id: [0; 32],
    });
    let tally_input = Input::Tally(TallyInput {
        output_index: 0,
        transaction_id: [0; 32],
    });
    let commit_input = Input::Commit(CommitInput {
        output_index: 0,
        poe: [0; 32],
        transaction_id: [0; 32],
    });
    let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
        pkh: Hash::SHA256([0; 32]),
        value: 0,
    });
    let data_request_output = Output::DataRequest(DataRequestOutput {
        backup_witnesses: 0,
        commit_fee: 0,
        data_request: [0; 32],
        reveal_fee: 0,
        tally_fee: 0,
        time_lock: 0,
        value: 0,
        witnesses: 0,
    });
    let commit_output = Output::Commit(CommitOutput {
        commitment: Hash::SHA256([0; 32]),
        value: 0,
    });
    let reveal_output = Output::Reveal(RevealOutput {
        pkh: Hash::SHA256([0; 32]),
        reveal: [0; 32],
        value: 0,
    });
    let consensus_output = Output::Consensus(ConsensusOutput {
        pkh: Hash::SHA256([0; 32]),
        result: [0; 32],
        value: 0,
    });

    let inputs = vec![
        value_transfer_input,
        reveal_input,
        tally_input,
        commit_input,
    ];
    let outputs = vec![
        value_transfer_output,
        data_request_output,
        commit_output,
        reveal_output,
        consensus_output,
    ];
    let txns: Vec<Transaction> = vec![Transaction {
        inputs,
        signatures: keyed_signatures,
        outputs,
        version: 0,
    }];

    // Expected message
    let msg = Message {
        kind: Command::Block(Block {
            block_header: block_header.clone(),
            proof: LeadershipProof {
                block_sig: Some(signature),
                influence: 0,
            },
            txns: txns.clone(),
        }),
        magic: MAGIC,
    };

    // Check that the build_block function builds the expected message
    assert_eq!(msg, Message::build_block(block_header, proof, txns));
}

#[test]
fn builders_build_get_peers() {
    // Expected message
    let msg = Message {
        kind: Command::GetPeers(GetPeers),
        magic: MAGIC,
    };

    // Check that the build_get_peers function builds the expected message
    assert_eq!(msg, Message::build_get_peers());
}

#[test]
fn builders_build_peers() {
    // Expected message
    let mut addresses = Vec::new();
    let address: Address = Address {
        ip: IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    addresses.push(address);
    let msg = Message {
        kind: Command::Peers(Peers { peers: addresses }),
        magic: MAGIC,
    };

    // Build vector of socket addresses
    let sock_addresses: Vec<SocketAddr> = vec!["192.168.1.1:8000".parse().unwrap()];

    // Check that the build_peers function builds the expected message
    assert_eq!(msg, Message::build_peers(&sock_addresses));
}

#[test]
fn builders_build_ping() {
    // Expected message (except nonce which is random)
    let msg = Message {
        kind: Command::Ping(Ping { nonce: 1234 }),
        magic: MAGIC,
    };

    // Build message
    let built_msg = Message::build_ping();

    // Check that the build_ping function builds the expected message
    assert_eq!(built_msg.magic, msg.magic);
    match built_msg.kind {
        Command::Ping(Ping { nonce: _ }) => assert!(true),
        _ => assert!(false, "Expected ping, found another type"),
    };
}

#[test]
fn builders_build_pong() {
    // Expected message
    let nonce = 1234;
    let msg = Message {
        kind: Command::Pong(Pong { nonce }),
        magic: MAGIC,
    };

    // Check that the build_pong function builds the expected message
    assert_eq!(msg, Message::build_pong(nonce));
}

#[test]
fn builders_build_version() {
    // Expected message (except nonce which is random and timestamp which is the current one)
    let hardcoded_last_epoch = 1234;
    let sender_addr = Address {
        ip: IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let receiver_addr = Address {
        ip: IpAddress::Ipv4 { ip: 3232235778 },
        port: 8001,
    };
    let version_cmd = Command::Version(Version {
        version: PROTOCOL_VERSION,
        timestamp: 1234,
        capabilities: CAPABILITIES,
        sender_address: sender_addr,
        receiver_address: receiver_addr,
        user_agent: USER_AGENT.to_string(),
        last_epoch: hardcoded_last_epoch,
        genesis: GENESIS,
        nonce: 1234,
    });
    let msg = Message {
        kind: version_cmd,
        magic: MAGIC,
    };

    // Build message
    let sender_sock_addr = "192.168.1.1:8000".parse().unwrap();
    let receiver_sock_addr = "192.168.1.2:8001".parse().unwrap();
    let built_msg =
        Message::build_version(sender_sock_addr, receiver_sock_addr, hardcoded_last_epoch);

    // Check that the build_version function builds the expected message
    assert_eq!(built_msg.magic, msg.magic);
    match &built_msg.kind {
        Command::Version(Version {
            version,
            timestamp: _,
            capabilities,
            sender_address,
            receiver_address,
            user_agent,
            last_epoch,
            genesis,
            nonce: _,
        }) if *version == PROTOCOL_VERSION
            && *capabilities == CAPABILITIES
            && *sender_address == sender_addr
            && *receiver_address == receiver_addr
            && user_agent == USER_AGENT
            && *last_epoch == hardcoded_last_epoch
            && *genesis == GENESIS =>
        {
            assert!(true)
        }
        _ => assert!(false, "Some field/s do not match the expected value"),
    };
}

#[test]
fn builders_build_verack() {
    // Expected message
    let msg = Message {
        kind: Command::Verack(Verack),
        magic: MAGIC,
    };

    // Check that the build_verack function builds the expected message
    assert_eq!(msg, Message::build_verack());
}

#[test]
fn builders_build_inventory_announcement() {
    // Inventory elements
    let inv_item_1 = InventoryEntry::Tx(Hash::SHA256([1; 32]));
    let inv_item_2 = InventoryEntry::Block(Hash::SHA256([2; 32]));
    let inventory = vec![inv_item_1, inv_item_2];

    // InventoryAnnouncement command
    let inv_cmd = Command::InventoryAnnouncement(InventoryAnnouncement {
        inventory: inventory.clone(),
    });

    // InventoryAnnouncement message
    let msg = Message {
        kind: inv_cmd,
        magic: MAGIC,
    };

    // Check that the build_inventory_announcement function builds the expected message
    assert_eq!(
        msg,
        Message::build_inventory_announcement(inventory).unwrap()
    );
}

#[test]
fn builders_build_inventory_request() {
    // Inventory elements
    let inv_item_1 = InventoryEntry::Tx(Hash::SHA256([1; 32]));
    let inv_item_2 = InventoryEntry::Block(Hash::SHA256([2; 32]));
    let inventory = vec![inv_item_1, inv_item_2];

    // InventoryRequest command
    let inv_req_cmd = Command::InventoryRequest(InventoryRequest {
        inventory: inventory.clone(),
    });

    // Inventory message
    let msg = Message {
        kind: inv_req_cmd,
        magic: MAGIC,
    };

    // Check that the build_inv function builds the expected message
    assert_eq!(msg, Message::build_inventory_request(inventory).unwrap());
}
