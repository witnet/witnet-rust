use std::net::SocketAddr;

use witnet_data_structures::{builders::*, chain::*, transaction::Transaction, types::*, vrf::*};

#[test]
fn builders_build_last_beacon() {
    let highest_block_checkpoint = CheckpointBeacon::default();
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
    let block_header = BlockHeader {
        version: 0x0000_0001,
        beacon: CheckpointBeacon::default(),
        merkle_roots: BlockMerkleRoots::default(),
        proof: BlockEligibilityClaim::default(),
        bn256_public_key: Default::default(),
    };
    let block_sig = KeyedSignature::default();

    let mut data_request_txns = vec![];
    if let Transaction::DataRequest(dr_tx) = transaction_example() {
        data_request_txns.push(dr_tx);
    }
    let txns = BlockTransactions {
        data_request_txns,
        ..BlockTransactions::default()
    };

    // Expected message
    let msg = Message {
        kind: Command::Block(Block {
            block_header: block_header.clone(),
            block_sig: KeyedSignature::default(),
            txns: txns.clone(),
        }),
        magic: 0xABCD,
    };

    // Check that the build_block function builds the expected message
    assert_eq!(
        msg,
        Message::build_block(0xABCD, block_header, block_sig, txns)
    );
}

#[test]
fn builders_build_transaction() {
    let txn = transaction_example();

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
        ip: IpAddress::Ipv4 { ip: 3_232_235_777 },
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
fn builders_build_version() {
    // Expected message (except nonce which is random and timestamp which is the current one)
    let hardcoded_beacon = LastBeacon {
        highest_block_checkpoint: CheckpointBeacon {
            hash_prev_block: Hash::SHA256([4; 32]),
            checkpoint: 7,
        },
    };
    let sender_addr = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_777 },
        port: 8000,
    };
    let receiver_addr = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_778 },
        port: 8001,
    };
    let version_cmd = Command::Version(Version {
        version: PROTOCOL_VERSION,
        timestamp: 1234,
        capabilities: CAPABILITIES,
        sender_address: sender_addr,
        receiver_address: receiver_addr,
        user_agent: USER_AGENT.to_string(),
        nonce: 1234,
        beacon: hardcoded_beacon.clone(),
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
        Some(sender_sock_addr),
        receiver_sock_addr,
        hardcoded_beacon.clone(),
    );

    // Check that the build_version function builds the expected message
    assert_eq!(built_msg.magic, msg.magic);
    match &built_msg.kind {
        Command::Version(Version {
            version,
            capabilities,
            sender_address,
            receiver_address,
            user_agent,
            beacon,
            ..
        }) => assert!(
            *version == PROTOCOL_VERSION
                && *capabilities == CAPABILITIES
                && *sender_address == sender_addr
                && *receiver_address == receiver_addr
                && user_agent == USER_AGENT
                && *beacon == hardcoded_beacon
        ),
        _ => panic!("Some field/s do not match the expected value"),
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
