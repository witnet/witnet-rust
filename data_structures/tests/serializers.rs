use witnet_data_structures::{
    proto::{
        versioning::{ProtocolVersion, Versioned},
        ProtobufConvert,
    },
    {chain::*, types::*},
};

const EXAMPLE_BLOCK_VECTOR_LEGACY: &[u8] = &[
    8, 1, 18, 165, 5, 42, 162, 5, 10, 172, 2, 18, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 216, 1, 10, 34, 10, 32,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 26, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 50, 34, 10, 32, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34,
    39, 10, 37, 18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 18, 41, 10, 2, 10, 0, 18, 35, 10, 33, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 197, 2,
    10, 0, 26, 192, 2, 10, 146, 2, 10, 38, 10, 36, 10, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 24, 10, 22, 10, 20, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 205, 1, 10, 202, 1, 18, 97, 8, 1, 18,
    93, 104, 116, 116, 112, 115, 58, 47, 47, 111, 112, 101, 110, 119, 101, 97, 116, 104, 101, 114,
    109, 97, 112, 46, 111, 114, 103, 47, 100, 97, 116, 97, 47, 50, 46, 53, 47, 119, 101, 97, 116,
    104, 101, 114, 63, 105, 100, 61, 50, 57, 53, 48, 49, 53, 57, 38, 97, 112, 112, 105, 100, 61,
    98, 54, 57, 48, 55, 100, 50, 56, 57, 101, 49, 48, 100, 55, 49, 52, 97, 54, 101, 56, 56, 98, 51,
    48, 55, 54, 49, 102, 97, 101, 50, 50, 18, 97, 8, 1, 18, 93, 104, 116, 116, 112, 115, 58, 47,
    47, 111, 112, 101, 110, 119, 101, 97, 116, 104, 101, 114, 109, 97, 112, 46, 111, 114, 103, 47,
    100, 97, 116, 97, 47, 50, 46, 53, 47, 119, 101, 97, 116, 104, 101, 114, 63, 105, 100, 61, 50,
    57, 53, 48, 49, 53, 57, 38, 97, 112, 112, 105, 100, 61, 98, 54, 57, 48, 55, 100, 50, 56, 57,
    101, 49, 48, 100, 55, 49, 52, 97, 54, 101, 56, 56, 98, 51, 48, 55, 54, 49, 102, 97, 101, 50,
    50, 26, 0, 34, 0, 18, 41, 10, 2, 10, 0, 18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

const EXAMPLE_BLOCK_VECTOR_TRANSITION: &[u8] = &[
    8, 1, 18, 237, 5, 42, 234, 5, 10, 244, 2, 18, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 160, 2, 10, 34, 10, 32,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 26, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 50, 34, 10, 32, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 58,
    34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 66, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 39, 10, 37, 18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 18, 41, 10, 2, 10, 0,
    18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 26, 197, 2, 10, 0, 26, 192, 2, 10, 146, 2, 10, 38, 10, 36, 10, 34, 10, 32,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    18, 24, 10, 22, 10, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 205, 1,
    10, 202, 1, 18, 97, 8, 1, 18, 93, 104, 116, 116, 112, 115, 58, 47, 47, 111, 112, 101, 110, 119,
    101, 97, 116, 104, 101, 114, 109, 97, 112, 46, 111, 114, 103, 47, 100, 97, 116, 97, 47, 50, 46,
    53, 47, 119, 101, 97, 116, 104, 101, 114, 63, 105, 100, 61, 50, 57, 53, 48, 49, 53, 57, 38, 97,
    112, 112, 105, 100, 61, 98, 54, 57, 48, 55, 100, 50, 56, 57, 101, 49, 48, 100, 55, 49, 52, 97,
    54, 101, 56, 56, 98, 51, 48, 55, 54, 49, 102, 97, 101, 50, 50, 18, 97, 8, 1, 18, 93, 104, 116,
    116, 112, 115, 58, 47, 47, 111, 112, 101, 110, 119, 101, 97, 116, 104, 101, 114, 109, 97, 112,
    46, 111, 114, 103, 47, 100, 97, 116, 97, 47, 50, 46, 53, 47, 119, 101, 97, 116, 104, 101, 114,
    63, 105, 100, 61, 50, 57, 53, 48, 49, 53, 57, 38, 97, 112, 112, 105, 100, 61, 98, 54, 57, 48,
    55, 100, 50, 56, 57, 101, 49, 48, 100, 55, 49, 52, 97, 54, 101, 56, 56, 98, 51, 48, 55, 54, 49,
    102, 97, 101, 50, 50, 26, 0, 34, 0, 18, 41, 10, 2, 10, 0, 18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

const EXAMPLE_BLOCK_VECTOR_FINAL: &[u8] = &[
    8, 1, 18, 237, 5, 42, 234, 5, 10, 244, 2, 18, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 160, 2, 10, 34, 10, 32,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 26, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 50, 34, 10, 32, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 58,
    34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 66, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 39, 10, 37, 18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 18, 41, 10, 2, 10, 0,
    18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 26, 197, 2, 10, 0, 26, 192, 2, 10, 146, 2, 10, 38, 10, 36, 10, 34, 10, 32,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    18, 24, 10, 22, 10, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 205, 1,
    10, 202, 1, 18, 97, 8, 1, 18, 93, 104, 116, 116, 112, 115, 58, 47, 47, 111, 112, 101, 110, 119,
    101, 97, 116, 104, 101, 114, 109, 97, 112, 46, 111, 114, 103, 47, 100, 97, 116, 97, 47, 50, 46,
    53, 47, 119, 101, 97, 116, 104, 101, 114, 63, 105, 100, 61, 50, 57, 53, 48, 49, 53, 57, 38, 97,
    112, 112, 105, 100, 61, 98, 54, 57, 48, 55, 100, 50, 56, 57, 101, 49, 48, 100, 55, 49, 52, 97,
    54, 101, 56, 56, 98, 51, 48, 55, 54, 49, 102, 97, 101, 50, 50, 18, 97, 8, 1, 18, 93, 104, 116,
    116, 112, 115, 58, 47, 47, 111, 112, 101, 110, 119, 101, 97, 116, 104, 101, 114, 109, 97, 112,
    46, 111, 114, 103, 47, 100, 97, 116, 97, 47, 50, 46, 53, 47, 119, 101, 97, 116, 104, 101, 114,
    63, 105, 100, 61, 50, 57, 53, 48, 49, 53, 57, 38, 97, 112, 112, 105, 100, 61, 98, 54, 57, 48,
    55, 100, 50, 56, 57, 101, 49, 48, 100, 55, 49, 52, 97, 54, 101, 56, 56, 98, 51, 48, 55, 54, 49,
    102, 97, 101, 50, 50, 26, 0, 34, 0, 18, 41, 10, 2, 10, 0, 18, 35, 10, 33, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

#[ignore]
#[test]
fn message_last_beacon_from_bytes() {
    let highest_superblock_checkpoint = CheckpointBeacon {
        checkpoint: 1,
        hash_prev_block: Hash::SHA256([1; 32]),
    };
    let buff: Vec<u8> = [
        18, 83, 66, 81, 10, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 41, 13, 1, 0, 0, 0, 18, 34, 10, 32, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    ]
    .to_vec();

    let expected_msg = Message {
        kind: Command::LastBeacon(LastBeacon {
            highest_block_checkpoint: CheckpointBeacon::default(),
            highest_superblock_checkpoint,
        }),
        magic: 0,
    };

    assert_eq!(Message::from_pb_bytes(&buff).unwrap(), expected_msg);
}

#[ignore]
#[test]
fn message_last_beacon_to_bytes() {
    let highest_superblock_checkpoint = CheckpointBeacon {
        checkpoint: 1,
        hash_prev_block: Hash::SHA256([1; 32]),
    };

    let msg = Message {
        kind: Command::LastBeacon(LastBeacon {
            highest_block_checkpoint: CheckpointBeacon::default(),
            highest_superblock_checkpoint,
        }),
        magic: 0,
    };
    let expected_buf: Vec<u8> = [
        18, 83, 66, 81, 10, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 41, 13, 1, 0, 0, 0, 18, 34, 10, 32, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    ]
    .to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(result, expected_buf);
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
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(msg, Message::from_pb_bytes(&result).unwrap());
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
    let addresses = vec![address];

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
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(msg, Message::from_pb_bytes(&result).unwrap());
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
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_version_to_bytes() {
    let beacon = LastBeacon {
        highest_block_checkpoint: CheckpointBeacon {
            hash_prev_block: Hash::SHA256([4; 32]),
            checkpoint: 7,
        },
        highest_superblock_checkpoint: CheckpointBeacon {
            hash_prev_block: Hash::SHA256([5; 32]),
            checkpoint: 1,
        },
    };
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
            nonce: 1,
            beacon,
            protocol_versions: vec![],
        }),
        magic: 1,
    };
    let expected_buf: Vec<u8> = [
        8, 1, 18, 139, 1, 10, 136, 1, 8, 2, 16, 123, 25, 4, 0, 0, 0, 0, 0, 0, 0, 34, 8, 10, 6, 192,
        168, 1, 1, 31, 64, 42, 8, 10, 6, 192, 168, 1, 2, 31, 65, 50, 4, 97, 115, 100, 102, 57, 1,
        0, 0, 0, 0, 0, 0, 0, 66, 86, 10, 41, 13, 7, 0, 0, 0, 18, 34, 10, 32, 4, 4, 4, 4, 4, 4, 4,
        4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 18, 41, 13, 1,
        0, 0, 0, 18, 34, 10, 32, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
        5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    ]
    .to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(result, expected_buf);
}

#[test]
fn message_version_from_bytes() {
    let beacon = LastBeacon {
        highest_block_checkpoint: CheckpointBeacon {
            hash_prev_block: Hash::SHA256([4; 32]),
            checkpoint: 7,
        },
        highest_superblock_checkpoint: CheckpointBeacon {
            hash_prev_block: Hash::SHA256([5; 32]),
            checkpoint: 1,
        },
    };
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
            nonce: 1,
            beacon,
            protocol_versions: vec![],
        }),
        magic: 1,
    };

    let buf: Vec<u8> = [
        8, 1, 18, 139, 1, 10, 136, 1, 8, 2, 16, 123, 25, 4, 0, 0, 0, 0, 0, 0, 0, 34, 8, 10, 6, 192,
        168, 1, 1, 31, 64, 42, 8, 10, 6, 192, 168, 1, 2, 31, 65, 50, 4, 97, 115, 100, 102, 57, 1,
        0, 0, 0, 0, 0, 0, 0, 66, 86, 10, 41, 13, 7, 0, 0, 0, 18, 34, 10, 32, 4, 4, 4, 4, 4, 4, 4,
        4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 18, 41, 13, 1,
        0, 0, 0, 18, 34, 10, 32, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
        5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
    ]
    .to_vec();

    assert_eq!(Message::from_pb_bytes(&buf).unwrap(), expected_msg);
}

#[test]
fn message_version_encode_decode() {
    let beacon = LastBeacon {
        highest_block_checkpoint: CheckpointBeacon {
            hash_prev_block: Hash::SHA256([4; 32]),
            checkpoint: 7,
        },
        highest_superblock_checkpoint: CheckpointBeacon {
            hash_prev_block: Hash::SHA256([5; 32]),
            checkpoint: 1,
        },
    };
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
            nonce: 1,
            beacon,
            protocol_versions: vec![],
        }),
        magic: 1,
    };
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(msg, Message::from_pb_bytes(&result).unwrap());
}

#[ignore]
#[test]
fn message_block_to_bytes() {
    let msg = Message {
        kind: Command::Block(block_example()),
        magic: 1,
    };

    let expected_buf: Vec<u8> = EXAMPLE_BLOCK_VECTOR_LEGACY.to_vec();
    let result: Vec<u8> = msg.to_versioned_pb_bytes(ProtocolVersion::V1_7).unwrap();
    assert_eq!(result, expected_buf);

    let expected_buf: Vec<u8> = EXAMPLE_BLOCK_VECTOR_TRANSITION.to_vec();
    let result: Vec<u8> = msg.to_versioned_pb_bytes(ProtocolVersion::V1_8).unwrap();
    assert_eq!(result, expected_buf);

    let expected_buf: Vec<u8> = EXAMPLE_BLOCK_VECTOR_FINAL.to_vec();
    let result: Vec<u8> = msg.to_versioned_pb_bytes(ProtocolVersion::V2_0).unwrap();
    assert_eq!(result, expected_buf);
}

#[ignore]
#[test]
fn message_block_from_bytes() {
    let expected_msg = Message {
        kind: Command::Block(block_example()),
        magic: 1,
    };

    assert_eq!(
        Message::from_versioned_pb_bytes(EXAMPLE_BLOCK_VECTOR_LEGACY).unwrap(),
        expected_msg
    );

    assert_eq!(
        Message::from_versioned_pb_bytes(EXAMPLE_BLOCK_VECTOR_TRANSITION).unwrap(),
        expected_msg
    );

    assert_eq!(
        Message::from_versioned_pb_bytes(EXAMPLE_BLOCK_VECTOR_FINAL).unwrap(),
        expected_msg
    );
}

#[ignore]
#[test]
fn message_block_encode_decode() {
    let msg = Message {
        kind: Command::Block(block_example()),
        magic: 1,
    };
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(msg, Message::from_pb_bytes(&result).unwrap());
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
        8, 1, 18, 78, 50, 76, 10, 36, 18, 34, 10, 32, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 10, 36, 10, 34, 10, 32, 2, 2, 2, 2, 2,
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
        8, 1, 18, 78, 50, 76, 10, 36, 18, 34, 10, 32, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 10, 36, 10, 34, 10, 32, 2, 2, 2, 2, 2,
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
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(msg, Message::from_pb_bytes(&result).unwrap());
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
        8, 1, 18, 78, 58, 76, 10, 36, 18, 34, 10, 32, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 10, 36, 10, 34, 10, 32, 2, 2, 2, 2, 2,
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
        8, 1, 18, 78, 58, 76, 10, 36, 18, 34, 10, 32, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 10, 36, 10, 34, 10, 32, 2, 2, 2, 2, 2,
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

    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(msg, Message::from_pb_bytes(&result).unwrap());
}

#[test]
fn message_transaction_encode_decode() {
    let msg = Message {
        kind: Command::Transaction(transaction_example()),
        magic: 1,
    };
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(msg, Message::from_pb_bytes(&result).unwrap());
}
