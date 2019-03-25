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
        magic: 0xABCD,
    };
    assert_eq!(
        msg,
        Message::build_last_beacon(0xABCD, highest_block_checkpoint)
    );
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
        signature: signature.clone(),
    }];
    let reveal_input = Input::Reveal(RevealInput {
        output_index: 0,
        transaction_id: Hash::SHA256([0; 32]),
    });
    let commit_input = Input::Commit(CommitInput {
        nonce: 0,
        output_index: 0,
        transaction_id: Hash::SHA256([0; 32]),
    });
    let data_request_input = Input::DataRequest(DataRequestInput {
        output_index: 0,
        poe: [0; 32],
        transaction_id: Hash::SHA256([0; 32]),
    });
    let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
        pkh: [0; 20],
        value: 0,
    });

    let rad_aggregate = RADAggregate { script: vec![0] };

    let rad_retrieve_1 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };

    let rad_retrieve_2 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };

    let rad_consensus = RADConsensus { script: vec![0] };

    let rad_deliver_1 = RADDeliver {
        kind: RADType::HttpGet,
        url: "https://hooks.zapier.com/hooks/catch/3860543/l2awcd/".to_string(),
    };

    let rad_deliver_2 = RADDeliver {
        kind: RADType::HttpGet,
        url: "https://hooks.zapier.com/hooks/catch/3860543/l1awcw/".to_string(),
    };

    let rad_request = RADRequest {
        aggregate: rad_aggregate,
        not_before: 0,
        retrieve: vec![rad_retrieve_1, rad_retrieve_2],
        consensus: rad_consensus,
        deliver: vec![rad_deliver_1, rad_deliver_2],
    };
    let data_request_output = Output::DataRequest(DataRequestOutput {
        backup_witnesses: 0,
        commit_fee: 0,
        data_request: rad_request,
        pkh: [0; 20],
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
        pkh: [0; 20],
        reveal: [0; 32].to_vec(),
        value: 0,
    });
    let consensus_output = Output::Tally(TallyOutput {
        pkh: [0; 20],
        result: [0; 32].to_vec(),
        value: 0,
    });

    let inputs = vec![reveal_input, data_request_input, commit_input];
    let outputs = vec![
        value_transfer_output,
        data_request_output,
        commit_output,
        reveal_output,
        consensus_output,
    ];
    let txns: Vec<Transaction> = vec![Transaction::new(
        TransactionBody::new(0, inputs, outputs),
        keyed_signatures,
    )];

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
        magic: 0xABCD,
    };

    // Check that the build_block function builds the expected message
    assert_eq!(msg, Message::build_block(0xABCD, block_header, proof, txns));
}

#[test]
fn builders_build_transaction() {
    let signature = Signature::Secp256k1(Secp256k1Signature {
        r: [0; 32],
        s: [0; 32],
        v: 0,
    });
    let keyed_signatures = vec![KeyedSignature {
        public_key: [0; 32],
        signature,
    }];
    let reveal_input = Input::Reveal(RevealInput {
        output_index: 0,
        transaction_id: Hash::SHA256([0; 32]),
    });
    let commit_input = Input::Commit(CommitInput {
        nonce: 0,
        output_index: 0,
        transaction_id: Hash::SHA256([0; 32]),
    });
    let data_request_input = Input::DataRequest(DataRequestInput {
        output_index: 0,
        poe: [0; 32],
        transaction_id: Hash::SHA256([0; 32]),
    });
    let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
        pkh: [0; 20],
        value: 0,
    });

    let rad_aggregate = RADAggregate { script: vec![0] };

    let rad_retrieve_1 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };

    let rad_retrieve_2 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };

    let rad_consensus = RADConsensus { script: vec![0] };

    let rad_deliver_1 = RADDeliver {
        kind: RADType::HttpGet,
        url: "https://hooks.zapier.com/hooks/catch/3860543/l2awcd/".to_string(),
    };

    let rad_deliver_2 = RADDeliver {
        kind: RADType::HttpGet,
        url: "https://hooks.zapier.com/hooks/catch/3860543/l1awcw/".to_string(),
    };

    let rad_request = RADRequest {
        aggregate: rad_aggregate,
        not_before: 0,
        retrieve: vec![rad_retrieve_1, rad_retrieve_2],
        consensus: rad_consensus,
        deliver: vec![rad_deliver_1, rad_deliver_2],
    };

    let data_request_output = Output::DataRequest(DataRequestOutput {
        backup_witnesses: 0,
        commit_fee: 0,
        data_request: rad_request,
        pkh: [0; 20],
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
        pkh: [0; 20],
        reveal: [0; 32].to_vec(),
        value: 0,
    });
    let consensus_output = Output::Tally(TallyOutput {
        pkh: [0; 20],
        result: [0; 32].to_vec(),
        value: 0,
    });

    let inputs = vec![reveal_input, data_request_input, commit_input];
    let outputs = vec![
        value_transfer_output,
        data_request_output,
        commit_output,
        reveal_output,
        consensus_output,
    ];

    let txn = Transaction::new(TransactionBody::new(0, inputs, outputs), keyed_signatures);

    // Expected message
    let msg = Message {
        kind: Command::Transaction(txn.clone()),
        magic: 0xABCD,
    };

    // Check that the build_transaction function builds the expected message
    assert_eq!(msg, Message::build_transaction(0xABCD, txn));
}

#[test]
fn builders_build_get_peers() {
    // Expected message
    let msg = Message {
        kind: Command::GetPeers(GetPeers),
        magic: 0xABCD,
    };

    // Check that the build_get_peers function builds the expected message
    assert_eq!(msg, Message::build_get_peers(0xABCD));
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
        magic: 0xABCD,
    };

    // Build vector of socket addresses
    let sock_addresses: Vec<SocketAddr> = vec!["192.168.1.1:8000".parse().unwrap()];

    // Check that the build_peers function builds the expected message
    assert_eq!(msg, Message::build_peers(0xABCD, &sock_addresses));
}

#[test]
fn builders_build_ping() {
    // Expected message (except nonce which is random)
    let msg = Message {
        kind: Command::Ping(Ping { nonce: 1234 }),
        magic: 0xABCD,
    };

    // Build message
    let built_msg = Message::build_ping(0xABCD);

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
        magic: 0xABCD,
    };

    // Check that the build_pong function builds the expected message
    assert_eq!(msg, Message::build_pong(0xABCD, nonce));
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
        nonce: 1234,
    });
    let msg = Message {
        kind: version_cmd,
        magic: 0xABCD,
    };

    // Build message
    let sender_sock_addr = "192.168.1.1:8000".parse().unwrap();
    let receiver_sock_addr = "192.168.1.2:8001".parse().unwrap();
    let built_msg = Message::build_version(
        0xABCD,
        sender_sock_addr,
        receiver_sock_addr,
        hardcoded_last_epoch,
    );

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
            nonce: _,
        }) if *version == PROTOCOL_VERSION
            && *capabilities == CAPABILITIES
            && *sender_address == sender_addr
            && *receiver_address == receiver_addr
            && user_agent == USER_AGENT
            && *last_epoch == hardcoded_last_epoch =>
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
        magic: 0xABCD,
    };

    // Check that the build_verack function builds the expected message
    assert_eq!(msg, Message::build_verack(0xABCD));
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
        magic: 0xABCD,
    };

    // Check that the build_inventory_announcement function builds the expected message
    assert_eq!(
        msg,
        Message::build_inventory_announcement(0xABCD, inventory).unwrap()
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
        magic: 0xABCD,
    };

    // Check that the build_inv function builds the expected message
    assert_eq!(
        msg,
        Message::build_inventory_request(0xABCD, inventory).unwrap()
    );
}
