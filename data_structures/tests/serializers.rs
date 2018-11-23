use witnet_data_structures::chain::*;
use witnet_data_structures::types::{
    Address, Command, GetPeers, IpAddress, Message, Peers, Ping, Pong, Verack, Version,
};

use witnet_data_structures::serializers::TryFrom;

#[test]
fn message_ping_to_bytes() {
    let msg = Message {
        kind: Command::Ping(Ping { nonce: 7 }),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 5, 12, 0, 0, 0, 0,
        0, 6, 0, 12, 0, 4, 0, 6, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0,
    ]
    .to_vec();
    let result: Vec<u8> = msg.into();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_ping_from_bytes() {
    let buff: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 5, 12, 0, 0, 0, 0,
        0, 6, 0, 12, 0, 4, 0, 6, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0,
    ]
    .to_vec();
    let expected_msg = Message {
        kind: Command::Ping(Ping { nonce: 7 }),
        magic: 0,
    };

    assert_eq!(Message::try_from(buff).unwrap(), expected_msg);
}

#[test]
fn message_ping_encode_decode() {
    let msg = Message {
        kind: Command::Ping(Ping { nonce: 5 }),
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, Message::try_from(result).unwrap());
}

#[test]
fn message_pong_to_bytes() {
    let msg = Message {
        kind: Command::Pong(Pong { nonce: 7 }),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 6, 12, 0, 0, 0, 0,
        0, 6, 0, 12, 0, 4, 0, 6, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0,
    ]
    .to_vec();
    let result: Vec<u8> = msg.into();
    assert_eq!(result, expected_buf);
}

#[test]
fn message_pong_from_bytes() {
    let buff: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 6, 12, 0, 0, 0, 0,
        0, 6, 0, 12, 0, 4, 0, 6, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0,
    ]
    .to_vec();
    let expected_msg = Message {
        kind: Command::Pong(Pong { nonce: 7 }),
        magic: 0,
    };

    assert_eq!(Message::try_from(buff).unwrap(), expected_msg);
}

#[test]
fn message_pong_encode_decode() {
    let msg = Message {
        kind: Command::Pong(Pong { nonce: 5 }),
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, Message::try_from(result).unwrap());
}

#[test]
fn message_get_peers_to_bytes() {
    let msg = Message {
        kind: Command::GetPeers(GetPeers),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 12, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 3, 8, 0, 0, 0, 4,
        0, 4, 0, 4, 0, 0, 0,
    ]
    .to_vec();
    let result: Vec<u8> = msg.into();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_get_peers_from_bytes() {
    let buf: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 12, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 3, 8, 0, 0, 0, 4,
        0, 4, 0, 4, 0, 0, 0,
    ]
    .to_vec();
    let expected_msg = Message {
        kind: Command::GetPeers(GetPeers),
        magic: 0,
    };

    assert_eq!(Message::try_from(buf).unwrap(), expected_msg);
}

#[test]
fn message_get_peers_encode_decode() {
    let msg = Message {
        kind: Command::GetPeers(GetPeers),
        magic: 0,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, Message::try_from(result).unwrap());
}

#[test]
fn message_get_peer_to_bytes() {
    let mut addresses = Vec::new();
    let address: Address = Address {
        ip: IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    addresses.push(address);
    let msg = Message {
        kind: Command::Peers(Peers { peers: addresses }),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 12, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 4, 4, 0, 0, 0,
        214, 255, 255, 255, 4, 0, 0, 0, 1, 0, 0, 0, 16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 5, 0, 8, 0, 6,
        0, 10, 0, 0, 0, 0, 1, 64, 31, 12, 0, 0, 0, 0, 0, 6, 0, 8, 0, 4, 0, 6, 0, 0, 0, 1, 1, 168,
        192,
    ]
    .to_vec();
    let result: Vec<u8> = msg.into();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_peer_from_bytes() {
    let buf: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 12, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 4, 4, 0, 0, 0,
        214, 255, 255, 255, 4, 0, 0, 0, 1, 0, 0, 0, 16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 5, 0, 8, 0, 6,
        0, 10, 0, 0, 0, 0, 1, 64, 31, 12, 0, 0, 0, 0, 0, 6, 0, 8, 0, 4, 0, 6, 0, 0, 0, 1, 1, 168,
        192,
    ]
    .to_vec();
    let address: Address = Address {
        ip: IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let mut addresses = Vec::new();

    addresses.push(address);

    let expected_msg = Message {
        kind: Command::Peers(Peers { peers: addresses }),
        magic: 0,
    };

    assert_eq!(Message::try_from(buf).unwrap(), expected_msg);
}

#[test]
fn message_get_peer_encode_decode() {
    let mut addresses = Vec::new();
    let address_ipv4: Address = Address {
        ip: IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let address_ipv6: Address = Address {
        ip: IpAddress::Ipv6 {
            ip0: 3232235777,
            ip1: 3232235776,
            ip2: 3232235778,
            ip3: 3232235777,
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
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, Message::try_from(result).unwrap());
}

#[test]
fn message_verack_to_bytes() {
    let msg = Message {
        kind: Command::Verack(Verack),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 12, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 2, 8, 0, 0, 0, 4,
        0, 4, 0, 4, 0, 0, 0,
    ]
    .to_vec();
    let result: Vec<u8> = msg.into();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_verack_from_bytes() {
    let buf: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 12, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 2, 8, 0, 0, 0, 4,
        0, 4, 0, 4, 0, 0, 0,
    ]
    .to_vec();
    let expected_msg = Message {
        kind: Command::Verack(Verack),
        magic: 0,
    };

    assert_eq!(Message::try_from(buf).unwrap(), expected_msg);
}

#[test]
fn message_verack_encode_decode() {
    let msg = Message {
        kind: Command::Verack(Verack),
        magic: 0,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, Message::try_from(result).unwrap());
}

#[test]
fn message_version_to_bytes() {
    let sender_address = Address {
        ip: IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let receiver_address = Address {
        ip: IpAddress::Ipv4 { ip: 3232235778 },
        port: 8001,
    };
    let msg = Message {
        kind: Command::Version(Version {
            version: 2,
            timestamp: 123,
            capabilities: 4,
            sender_address: sender_address,
            receiver_address: receiver_address,
            user_agent: "asdf".to_string(),
            last_epoch: 8,
            genesis: 2,
            nonce: 1,
        }),
        magic: 1,
    };
    let expected_buf: Vec<u8> = [
        20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 0, 14, 0, 6, 0, 5, 0, 8, 0, 10, 0, 0, 0, 0, 1, 1, 0, 28,
        0, 0, 0, 0, 0, 22, 0, 56, 0, 4, 0, 24, 0, 32, 0, 8, 0, 12, 0, 16, 0, 20, 0, 40, 0, 48, 0,
        22, 0, 0, 0, 2, 0, 0, 0, 100, 0, 0, 0, 56, 0, 0, 0, 40, 0, 0, 0, 8, 0, 0, 0, 123, 0, 0, 0,
        0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 4, 0,
        0, 0, 97, 115, 100, 102, 0, 0, 0, 0, 226, 255, 255, 255, 0, 1, 65, 31, 12, 0, 0, 0, 0, 0,
        6, 0, 10, 0, 4, 0, 6, 0, 0, 0, 2, 1, 168, 192, 0, 0, 10, 0, 14, 0, 5, 0, 8, 0, 6, 0, 10, 0,
        0, 0, 0, 1, 64, 31, 12, 0, 0, 0, 0, 0, 6, 0, 8, 0, 4, 0, 6, 0, 0, 0, 1, 1, 168, 192,
    ]
    .to_vec();
    let result: Vec<u8> = msg.into();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_version_from_bytes() {
    let sender_address = Address {
        ip: IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let receiver_address = Address {
        ip: IpAddress::Ipv4 { ip: 3232235778 },
        port: 8001,
    };
    let expected_msg = Message {
        kind: Command::Version(Version {
            version: 2,
            timestamp: 123,
            capabilities: 4,
            sender_address: sender_address,
            receiver_address: receiver_address,
            user_agent: "asdf".to_string(),
            last_epoch: 8,
            genesis: 2,
            nonce: 1,
        }),
        magic: 1,
    };
    let buf: Vec<u8> = [
        20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 0, 14, 0, 6, 0, 5, 0, 8, 0, 10, 0, 0, 0, 0, 1, 1, 0, 28,
        0, 0, 0, 0, 0, 22, 0, 56, 0, 4, 0, 24, 0, 32, 0, 8, 0, 12, 0, 16, 0, 20, 0, 40, 0, 48, 0,
        22, 0, 0, 0, 2, 0, 0, 0, 100, 0, 0, 0, 56, 0, 0, 0, 40, 0, 0, 0, 8, 0, 0, 0, 123, 0, 0, 0,
        0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 4, 0,
        0, 0, 97, 115, 100, 102, 0, 0, 0, 0, 226, 255, 255, 255, 0, 1, 65, 31, 12, 0, 0, 0, 0, 0,
        6, 0, 10, 0, 4, 0, 6, 0, 0, 0, 2, 1, 168, 192, 0, 0, 10, 0, 14, 0, 5, 0, 8, 0, 6, 0, 10, 0,
        0, 0, 0, 1, 64, 31, 12, 0, 0, 0, 0, 0, 6, 0, 8, 0, 4, 0, 6, 0, 0, 0, 1, 1, 168, 192,
    ]
    .to_vec();

    assert_eq!(Message::try_from(buf).unwrap(), expected_msg);
}

#[test]
fn message_version_encode_decode() {
    let sender_address = Address {
        ip: IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let receiver_address = Address {
        ip: IpAddress::Ipv4 { ip: 3232235778 },
        port: 8001,
    };
    let msg = Message {
        kind: Command::Version(Version {
            version: 2,
            timestamp: 123,
            capabilities: 4,
            sender_address: sender_address,
            receiver_address: receiver_address,
            user_agent: "asdf".to_string(),
            last_epoch: 8,
            genesis: 2,
            nonce: 1,
        }),
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, Message::try_from(result).unwrap());
}

#[test]
fn message_block_to_bytes() {
    let header = BlockHeader {
        version: 0,
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
    let header_with_proof = BlockHeaderWithProof {
        version: header.version,
        beacon: header.beacon,
        hash_merkle_root: header.hash_merkle_root,
        proof: LeadershipProof {
            block_sig: Some(signature),
            influence: 0,
        },
    };
    let txns: Vec<Transaction> = vec![Transaction];
    let msg = Message {
        kind: Command::Block(Block {
            header: header_with_proof.clone(),
            txn_count: txns.len() as u32,
            txns: txns.clone(),
        }),
        magic: 1,
    };

    let expected_buf: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 6, 0, 5, 0, 8, 0, 10, 0, 0, 0, 0, 7, 1, 0, 16, 0, 0, 0, 0,
        0, 10, 0, 16, 0, 4, 0, 8, 0, 12, 0, 10, 0, 0, 0, 40, 0, 0, 0, 1, 0, 0, 0, 4, 0, 0, 0, 1, 0,
        0, 0, 8, 0, 0, 0, 4, 0, 4, 0, 4, 0, 0, 0, 12, 0, 16, 0, 0, 0, 4, 0, 8, 0, 12, 0, 12, 0, 0,
        0, 172, 0, 0, 0, 124, 0, 0, 0, 12, 0, 0, 0, 8, 0, 12, 0, 7, 0, 8, 0, 8, 0, 0, 0, 0, 0, 0,
        1, 12, 0, 0, 0, 8, 0, 12, 0, 4, 0, 8, 0, 8, 0, 0, 0, 48, 0, 0, 0, 4, 0, 0, 0, 33, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 204, 255, 255, 255, 4, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 248,
        255, 255, 255, 12, 0, 0, 0, 8, 0, 8, 0, 0, 0, 4, 0, 8, 0, 0, 0, 4, 0, 0, 0, 32, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0,
    ]
    .to_vec();
    let result: Vec<u8> = msg.into();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_block_from_bytes() {
    let buf: Vec<u8> = [
        16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 6, 0, 5, 0, 8, 0, 10, 0, 0, 0, 0, 7, 1, 0, 16, 0, 0, 0, 0,
        0, 10, 0, 16, 0, 4, 0, 8, 0, 12, 0, 10, 0, 0, 0, 40, 0, 0, 0, 1, 0, 0, 0, 4, 0, 0, 0, 1, 0,
        0, 0, 8, 0, 0, 0, 4, 0, 4, 0, 4, 0, 0, 0, 12, 0, 16, 0, 0, 0, 4, 0, 8, 0, 12, 0, 12, 0, 0,
        0, 172, 0, 0, 0, 124, 0, 0, 0, 12, 0, 0, 0, 8, 0, 12, 0, 7, 0, 8, 0, 8, 0, 0, 0, 0, 0, 0,
        1, 12, 0, 0, 0, 8, 0, 12, 0, 4, 0, 8, 0, 8, 0, 0, 0, 48, 0, 0, 0, 4, 0, 0, 0, 33, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 204, 255, 255, 255, 4, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 248,
        255, 255, 255, 12, 0, 0, 0, 8, 0, 8, 0, 0, 0, 4, 0, 8, 0, 0, 0, 4, 0, 0, 0, 32, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0,
    ]
    .to_vec();

    let header = BlockHeader {
        version: 0,
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
    let header_with_proof = BlockHeaderWithProof {
        version: header.version,
        beacon: header.beacon,
        hash_merkle_root: header.hash_merkle_root,
        proof: LeadershipProof {
            block_sig: Some(signature),
            influence: 0,
        },
    };
    let txns: Vec<Transaction> = vec![Transaction];
    let expected_msg = Message {
        kind: Command::Block(Block {
            header: header_with_proof.clone(),
            txn_count: txns.len() as u32,
            txns: txns.clone(),
        }),
        magic: 1,
    };

    assert_eq!(Message::try_from(buf).unwrap(), expected_msg);
}

#[test]
fn message_block_encode_decode() {
    let header = BlockHeader {
        version: 0,
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
    let header_with_proof = BlockHeaderWithProof {
        version: header.version,
        beacon: header.beacon,
        hash_merkle_root: header.hash_merkle_root,
        proof: LeadershipProof {
            block_sig: Some(signature),
            influence: 0,
        },
    };
    let txns: Vec<Transaction> = vec![Transaction];
    let msg = Message {
        kind: Command::Block(Block {
            header: header_with_proof.clone(),
            txn_count: txns.len() as u32,
            txns: txns.clone(),
        }),
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, Message::try_from(result).unwrap());
}
