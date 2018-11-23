use std::net::SocketAddr;

use witnet_data_structures::builders::*;
use witnet_data_structures::types::*;

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
