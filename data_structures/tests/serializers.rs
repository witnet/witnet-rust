use witnet_data_structures::types;

use witnet_data_structures::serializers::MyTryFrom;

#[test]
fn message_ping_to_bytes() {
    let msg = types::Message {
        kind: types::Command::Ping { nonce: 7 },
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
    let expected_msg = types::Message {
        kind: types::Command::Ping { nonce: 7 },
        magic: 0,
    };

    assert_eq!(types::Message::try_from(buff).unwrap(), expected_msg);
}

#[test]
fn message_ping_encode_decode() {
    let msg = types::Message {
        kind: types::Command::Ping { nonce: 5 },
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, types::Message::try_from(result).unwrap());
}

#[test]
fn message_pong_to_bytes() {
    let msg = types::Message {
        kind: types::Command::Pong { nonce: 7 },
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
    let expected_msg = types::Message {
        kind: types::Command::Pong { nonce: 7 },
        magic: 0,
    };

    assert_eq!(types::Message::try_from(buff).unwrap(), expected_msg);
}

#[test]
fn message_pong_encode_decode() {
    let msg = types::Message {
        kind: types::Command::Pong { nonce: 5 },
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, types::Message::try_from(result).unwrap());
}

#[test]
fn message_get_peers_to_bytes() {
    let msg = types::Message {
        kind: types::Command::GetPeers,
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
    let expected_msg = types::Message {
        kind: types::Command::GetPeers,
        magic: 0,
    };

    assert_eq!(types::Message::try_from(buf).unwrap(), expected_msg);
}

#[test]
fn message_get_peers_encode_decode() {
    let msg = types::Message {
        kind: types::Command::GetPeers,
        magic: 0,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, types::Message::try_from(result).unwrap());
}

#[test]
fn message_get_peer_to_bytes() {
    let mut addresses = Vec::new();
    let address: types::Address = types::Address {
        ip: types::IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    addresses.push(address);
    let msg = types::Message {
        kind: types::Command::Peers { peers: addresses },
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
    let address: types::Address = types::Address {
        ip: types::IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let mut addresses = Vec::new();

    addresses.push(address);

    let expected_msg = types::Message {
        kind: types::Command::Peers { peers: addresses },
        magic: 0,
    };

    assert_eq!(types::Message::try_from(buf).unwrap(), expected_msg);
}

#[test]
fn message_get_peer_encode_decode() {
    let mut addresses = Vec::new();
    let address_ipv4: types::Address = types::Address {
        ip: types::IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let address_ipv6: types::Address = types::Address {
        ip: types::IpAddress::Ipv6 {
            ip0: 3232235777,
            ip1: 3232235776,
            ip2: 3232235778,
            ip3: 3232235777,
        },
        port: 8000,
    };

    addresses.push(address_ipv4);
    addresses.push(address_ipv6);

    let msg = types::Message {
        kind: types::Command::Peers { peers: addresses },
        magic: 0,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, types::Message::try_from(result).unwrap());
}

#[test]
fn message_verack_to_bytes() {
    let msg = types::Message {
        kind: types::Command::Verack,
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
    let expected_msg = types::Message {
        kind: types::Command::Verack,
        magic: 0,
    };

    assert_eq!(types::Message::try_from(buf).unwrap(), expected_msg);
}

#[test]
fn message_verack_encode_decode() {
    let msg = types::Message {
        kind: types::Command::Verack,
        magic: 0,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, types::Message::try_from(result).unwrap());
}

#[test]
fn message_version_to_bytes() {
    let sender_address = types::Address {
        ip: types::IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let receiver_address = types::Address {
        ip: types::IpAddress::Ipv4 { ip: 3232235778 },
        port: 8001,
    };
    let msg = types::Message {
        kind: types::Command::Version {
            version: 2,
            timestamp: 123,
            capabilities: 4,
            sender_address: sender_address,
            receiver_address: receiver_address,
            user_agent: "asdf".to_string(),
            last_epoch: 8,
            genesis: 2,
            nonce: 1,
        },
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
    let sender_address = types::Address {
        ip: types::IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let receiver_address = types::Address {
        ip: types::IpAddress::Ipv4 { ip: 3232235778 },
        port: 8001,
    };
    let expected_msg = types::Message {
        kind: types::Command::Version {
            version: 2,
            timestamp: 123,
            capabilities: 4,
            sender_address: sender_address,
            receiver_address: receiver_address,
            user_agent: "asdf".to_string(),
            last_epoch: 8,
            genesis: 2,
            nonce: 1,
        },
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

    assert_eq!(types::Message::try_from(buf).unwrap(), expected_msg);
}

#[test]
fn message_version_encode_decode() {
    let sender_address = types::Address {
        ip: types::IpAddress::Ipv4 { ip: 3232235777 },
        port: 8000,
    };
    let receiver_address = types::Address {
        ip: types::IpAddress::Ipv4 { ip: 3232235778 },
        port: 8001,
    };
    let msg = types::Message {
        kind: types::Command::Version {
            version: 2,
            timestamp: 123,
            capabilities: 4,
            sender_address: sender_address,
            receiver_address: receiver_address,
            user_agent: "asdf".to_string(),
            last_epoch: 8,
            genesis: 2,
            nonce: 1,
        },
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.into();

    assert_eq!(cloned_msg, types::Message::try_from(result).unwrap());
}
