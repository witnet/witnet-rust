use witnet_data_structures::{
    proto::ProtobufConvert,
    {chain::*, types::*},
};

const EXAMPLE_BLOCK_VECTOR: &[u8] = &[
    8, 1, 18, 169, 6, 58, 166, 6, 10, 170, 2, 18, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 216, 1, 10, 34, 10, 32,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 26, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 50, 34, 10, 32, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34,
    39, 10, 37, 18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 41, 10, 2, 10, 0, 18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 203, 3, 10, 26,
    18, 24, 10, 22, 10, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 172, 3,
    10, 254, 2, 10, 38, 10, 36, 10, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 24, 10, 22, 10, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 185, 2, 10, 182, 2, 18, 95, 18, 93, 104, 116, 116, 112,
    115, 58, 47, 47, 111, 112, 101, 110, 119, 101, 97, 116, 104, 101, 114, 109, 97, 112, 46, 111,
    114, 103, 47, 100, 97, 116, 97, 47, 50, 46, 53, 47, 119, 101, 97, 116, 104, 101, 114, 63, 105,
    100, 61, 50, 57, 53, 48, 49, 53, 57, 38, 97, 112, 112, 105, 100, 61, 98, 54, 57, 48, 55, 100,
    50, 56, 57, 101, 49, 48, 100, 55, 49, 52, 97, 54, 101, 56, 56, 98, 51, 48, 55, 54, 49, 102, 97,
    101, 50, 50, 18, 95, 18, 93, 104, 116, 116, 112, 115, 58, 47, 47, 111, 112, 101, 110, 119, 101,
    97, 116, 104, 101, 114, 109, 97, 112, 46, 111, 114, 103, 47, 100, 97, 116, 97, 47, 50, 46, 53,
    47, 119, 101, 97, 116, 104, 101, 114, 63, 105, 100, 61, 50, 57, 53, 48, 49, 53, 57, 38, 97,
    112, 112, 105, 100, 61, 98, 54, 57, 48, 55, 100, 50, 56, 57, 101, 49, 48, 100, 55, 49, 52, 97,
    54, 101, 56, 56, 98, 51, 48, 55, 54, 49, 102, 97, 101, 50, 50, 26, 0, 34, 0, 42, 54, 18, 52,
    104, 116, 116, 112, 115, 58, 47, 47, 104, 111, 111, 107, 115, 46, 122, 97, 112, 105, 101, 114,
    46, 99, 111, 109, 47, 104, 111, 111, 107, 115, 47, 99, 97, 116, 99, 104, 47, 51, 56, 54, 48,
    53, 52, 51, 47, 108, 50, 97, 119, 99, 100, 47, 42, 54, 18, 52, 104, 116, 116, 112, 115, 58, 47,
    47, 104, 111, 111, 107, 115, 46, 122, 97, 112, 105, 101, 114, 46, 99, 111, 109, 47, 104, 111,
    111, 107, 115, 47, 99, 97, 116, 99, 104, 47, 51, 56, 54, 48, 53, 52, 51, 47, 108, 49, 97, 119,
    99, 119, 47, 18, 41, 10, 2, 10, 0, 18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[test]
fn message_last_beacon_from_bytes() {
    let buff: Vec<u8> = [
        18, 40, 82, 38, 10, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ]
    .to_vec();

    let expected_msg = Message {
        kind: Command::LastBeacon(LastBeacon {
            highest_block_checkpoint: CheckpointBeacon::default(),
        }),
        magic: 0,
    };

    assert_eq!(Message::from_pb_bytes(&buff).unwrap(), expected_msg);
}

#[test]
fn message_last_beacon_to_bytes() {
    let msg = Message {
        kind: Command::LastBeacon(LastBeacon {
            highest_block_checkpoint: CheckpointBeacon::default(),
        }),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [
        18, 40, 82, 38, 10, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ]
    .to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_ping_to_bytes() {
    let msg = Message {
        kind: Command::Ping(Ping { nonce: 7 }),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [18, 11, 42, 9, 9, 7, 0, 0, 0, 0, 0, 0, 0].to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_ping_from_bytes() {
    let buff: Vec<u8> = [18, 11, 42, 9, 9, 7, 0, 0, 0, 0, 0, 0, 0].to_vec();
    let expected_msg = Message {
        kind: Command::Ping(Ping { nonce: 7 }),
        magic: 0,
    };

    assert_eq!(Message::from_pb_bytes(&buff).unwrap(), expected_msg);
}

#[test]
fn message_ping_encode_decode() {
    let msg = Message {
        kind: Command::Ping(Ping { nonce: 5 }),
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_pong_to_bytes() {
    let msg = Message {
        kind: Command::Pong(Pong { nonce: 7 }),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [18, 11, 50, 9, 9, 7, 0, 0, 0, 0, 0, 0, 0].to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();
    assert_eq!(result, expected_buf);
}

#[test]
fn message_pong_from_bytes() {
    let buff: Vec<u8> = [18, 11, 50, 9, 9, 7, 0, 0, 0, 0, 0, 0, 0].to_vec();
    let expected_msg = Message {
        kind: Command::Pong(Pong { nonce: 7 }),
        magic: 0,
    };

    assert_eq!(Message::from_pb_bytes(&buff).unwrap(), expected_msg);
}

#[test]
fn message_pong_encode_decode() {
    let msg = Message {
        kind: Command::Pong(Pong { nonce: 5 }),
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_get_peers_to_bytes() {
    let msg = Message {
        kind: Command::GetPeers(GetPeers),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [18, 2, 26, 0].to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_get_peers_from_bytes() {
    let buf: Vec<u8> = [18, 2, 26, 0].to_vec();
    let expected_msg = Message {
        kind: Command::GetPeers(GetPeers),
        magic: 0,
    };

    assert_eq!(Message::from_pb_bytes(&buf).unwrap(), expected_msg);
}

#[test]
fn message_get_peers_encode_decode() {
    let msg = Message {
        kind: Command::GetPeers(GetPeers),
        magic: 0,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_get_peer_to_bytes() {
    let mut addresses = Vec::new();
    let address: Address = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_777 },
        port: 8000,
    };
    addresses.push(address);
    let msg = Message {
        kind: Command::Peers(Peers { peers: addresses }),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [18, 12, 34, 10, 10, 8, 10, 6, 192, 168, 1, 1, 31, 64].to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_peer_from_bytes() {
    let buf: Vec<u8> = [18, 12, 34, 10, 10, 8, 10, 6, 192, 168, 1, 1, 31, 64].to_vec();
    let address: Address = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_777 },
        port: 8000,
    };
    let mut addresses = Vec::new();

    addresses.push(address);

    let expected_msg = Message {
        kind: Command::Peers(Peers { peers: addresses }),
        magic: 0,
    };

    assert_eq!(Message::from_pb_bytes(&buf).unwrap(), expected_msg);
}

#[test]
fn message_get_peer_encode_decode() {
    let mut addresses = Vec::new();
    let address_ipv4: Address = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_777 },
        port: 8000,
    };
    let address_ipv6: Address = Address {
        ip: IpAddress::Ipv6 {
            ip0: 3_232_235_777,
            ip1: 3_232_235_776,
            ip2: 3_232_235_778,
            ip3: 3_232_235_777,
        },
        port: 8000,
    };

    addresses.push(address_ipv4);
    addresses.push(address_ipv6);

    let msg = Message {
        kind: Command::Peers(Peers { peers: addresses }),
        magic: 0,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_verack_to_bytes() {
    let msg = Message {
        kind: Command::Verack(Verack),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [18, 2, 18, 0].to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_verack_from_bytes() {
    let buf: Vec<u8> = [18, 2, 18, 0].to_vec();
    let expected_msg = Message {
        kind: Command::Verack(Verack),
        magic: 0,
    };

    assert_eq!(Message::from_pb_bytes(&buf).unwrap(), expected_msg);
}

#[test]
fn message_verack_encode_decode() {
    let msg = Message {
        kind: Command::Verack(Verack),
        magic: 0,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_version_to_bytes() {
    let sender_address = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_777 },
        port: 8000,
    };
    let receiver_address = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_778 },
        port: 8001,
    };
    let msg = Message {
        kind: Command::Version(Version {
            version: 2,
            timestamp: 123,
            capabilities: 4,
            sender_address,
            receiver_address,
            user_agent: "asdf".to_string(),
            last_epoch: 8,
            nonce: 1,
        }),
        magic: 1,
    };
    let expected_buf: Vec<u8> = [
        8, 1, 18, 55, 10, 53, 8, 2, 16, 123, 25, 4, 0, 0, 0, 0, 0, 0, 0, 34, 8, 10, 6, 192, 168, 1,
        1, 31, 64, 42, 8, 10, 6, 192, 168, 1, 2, 31, 65, 50, 4, 97, 115, 100, 102, 61, 8, 0, 0, 0,
        65, 1, 0, 0, 0, 0, 0, 0, 0,
    ]
    .to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_version_from_bytes() {
    let sender_address = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_777 },
        port: 8000,
    };
    let receiver_address = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_778 },
        port: 8001,
    };
    let expected_msg = Message {
        kind: Command::Version(Version {
            version: 2,
            timestamp: 123,
            capabilities: 4,
            sender_address,
            receiver_address,
            user_agent: "asdf".to_string(),
            last_epoch: 8,
            nonce: 1,
        }),
        magic: 1,
    };

    let buf: Vec<u8> = [
        8, 1, 18, 55, 10, 53, 8, 2, 16, 123, 25, 4, 0, 0, 0, 0, 0, 0, 0, 34, 8, 10, 6, 192, 168, 1,
        1, 31, 64, 42, 8, 10, 6, 192, 168, 1, 2, 31, 65, 50, 4, 97, 115, 100, 102, 61, 8, 0, 0, 0,
        65, 1, 0, 0, 0, 0, 0, 0, 0,
    ]
    .to_vec();

    assert_eq!(Message::from_pb_bytes(&buf).unwrap(), expected_msg);
}

#[test]
fn message_version_encode_decode() {
    let sender_address = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_777 },
        port: 8000,
    };
    let receiver_address = Address {
        ip: IpAddress::Ipv4 { ip: 3_232_235_778 },
        port: 8001,
    };
    let msg = Message {
        kind: Command::Version(Version {
            version: 2,
            timestamp: 123,
            capabilities: 4,
            sender_address,
            receiver_address,
            user_agent: "asdf".to_string(),
            last_epoch: 8,
            nonce: 1,
        }),
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_block_to_bytes() {
    let msg = Message {
        kind: Command::Block(block_example()),
        magic: 1,
    };

    let expected_buf: Vec<u8> = EXAMPLE_BLOCK_VECTOR.to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();
    assert_eq!(result, expected_buf);
}

#[test]
fn message_block_from_bytes() {
    let expected_msg = Message {
        kind: Command::Block(block_example()),
        magic: 1,
    };

    assert_eq!(
        Message::from_pb_bytes(EXAMPLE_BLOCK_VECTOR).unwrap(),
        expected_msg
    );
}

#[test]
fn message_block_encode_decode() {
    let msg = Message {
        kind: Command::Block(block_example()),
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_inv_to_bytes() {
    // Inventory elements
    let inv_item_1 = InventoryEntry::Tx(Hash::SHA256([1; 32]));
    let inv_item_2 = InventoryEntry::Block(Hash::SHA256([2; 32]));

    // Inventory message
    let msg = Message {
        kind: Command::InventoryAnnouncement(InventoryAnnouncement {
            inventory: vec![inv_item_1, inv_item_2],
        }),
        magic: 1,
    };

    // Expected bytes
    let expected_buf: Vec<u8> = [
        8, 1, 18, 78, 66, 76, 10, 36, 18, 34, 10, 32, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 10, 36, 26, 34, 10, 32, 2, 2, 2, 2, 2,
        2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    ]
    .to_vec();

    // Serialize message to bytes
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    // Test check
    assert_eq!(result, expected_buf);
}

#[test]
fn message_inv_from_bytes() {
    // Inventory elements
    let inv_item_1 = InventoryEntry::Tx(Hash::SHA256([1; 32]));
    let inv_item_2 = InventoryEntry::Block(Hash::SHA256([2; 32]));

    // Inventory message
    let expected_msg = Message {
        kind: Command::InventoryAnnouncement(InventoryAnnouncement {
            inventory: vec![inv_item_1, inv_item_2],
        }),
        magic: 1,
    };
    let buf: Vec<u8> = [
        8, 1, 18, 78, 66, 76, 10, 36, 18, 34, 10, 32, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 10, 36, 26, 34, 10, 32, 2, 2, 2, 2, 2,
        2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    ]
    .to_vec();

    assert_eq!(Message::from_pb_bytes(&buf).unwrap(), expected_msg);
}

#[test]
fn message_inv_encode_decode() {
    // Inventory elements
    let inv_item_1 = InventoryEntry::Tx(Hash::SHA256([1; 32]));
    let inv_item_2 = InventoryEntry::Block(Hash::SHA256([2; 32]));

    // Inventory message
    let msg = Message {
        kind: Command::InventoryAnnouncement(InventoryAnnouncement {
            inventory: vec![inv_item_1, inv_item_2],
        }),
        magic: 1,
    };

    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_get_data_to_bytes() {
    // Inventory elements
    let inv_elem_1 = InventoryEntry::Tx(Hash::SHA256([1; 32]));
    let inv_elem_2 = InventoryEntry::Block(Hash::SHA256([2; 32]));

    // InventoryRequest message
    let msg = Message {
        kind: Command::InventoryRequest(InventoryRequest {
            inventory: vec![inv_elem_1, inv_elem_2],
        }),
        magic: 1,
    };

    // Expected bytes
    let expected_buf: Vec<u8> = [
        8, 1, 18, 78, 74, 76, 10, 36, 18, 34, 10, 32, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 10, 36, 26, 34, 10, 32, 2, 2, 2, 2, 2,
        2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    ]
    .to_vec();

    // Serialize message to bytes
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    // Test check
    assert_eq!(result, expected_buf);
}

#[test]
fn message_get_data_from_bytes() {
    // Inventory elements
    let inv_elem_1 = InventoryEntry::Tx(Hash::SHA256([1; 32]));
    let inv_elem_2 = InventoryEntry::Block(Hash::SHA256([2; 32]));

    // Inventory message
    let expected_msg = Message {
        kind: Command::InventoryRequest(InventoryRequest {
            inventory: vec![inv_elem_1, inv_elem_2],
        }),
        magic: 1,
    };
    let buf: Vec<u8> = [
        8, 1, 18, 78, 74, 76, 10, 36, 18, 34, 10, 32, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 10, 36, 26, 34, 10, 32, 2, 2, 2, 2, 2,
        2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
    ]
    .to_vec();

    assert_eq!(Message::from_pb_bytes(&buf).unwrap(), expected_msg);
}

#[test]
fn message_get_data_encode_decode() {
    // Inventory elements
    let inv_elem_1 = InventoryEntry::Tx(Hash::SHA256([1; 32]));
    let inv_elem_2 = InventoryEntry::Block(Hash::SHA256([2; 32]));

    // Inventory message
    let msg = Message {
        kind: Command::InventoryRequest(InventoryRequest {
            inventory: vec![inv_elem_1, inv_elem_2],
        }),
        magic: 1,
    };

    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_transaction_encode_decode() {
    let msg = Message {
        kind: Command::Transaction(transaction_example()),
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}
