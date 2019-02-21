use witnet_data_structures::{
    proto::ProtobufConvert,
    {chain::*, types::*},
};

#[test]
fn message_get_blocks_from_bytes() {
    let buff: Vec<u8> = [
        18, 40, 82, 38, 10, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ]
    .to_vec();

    let expected_msg = Message {
        kind: Command::LastBeacon(LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: Hash::SHA256([0; 32]),
                checkpoint: 0,
            },
        }),
        magic: 0,
    };

    assert_eq!(Message::from_pb_bytes(&buff).unwrap(), expected_msg);
}

#[test]
fn message_get_blocks_to_bytes() {
    let msg = Message {
        kind: Command::LastBeacon(LastBeacon {
            highest_block_checkpoint: CheckpointBeacon {
                hash_prev_block: Hash::SHA256([0; 32]),
                checkpoint: 0,
            },
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
        ip: IpAddress::Ipv4 { ip: 3232235777 },
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
        ip: IpAddress::Ipv4 { ip: 3232235777 },
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
    let block_header = BlockHeader {
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
    let proof = LeadershipProof {
        block_sig: Some(signature.clone()),
        influence: 0,
    };
    let keyed_signature = vec![KeyedSignature {
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
        reveal: [0; 32].to_vec(),
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
    let inputs = vec![commit_input, data_request_input, reveal_input];
    let outputs = vec![
        value_transfer_output,
        data_request_output,
        commit_output,
        reveal_output,
        consensus_output,
    ];
    let txns: Vec<Transaction> = vec![Transaction {
        inputs,
        signatures: keyed_signature,
        outputs,
        version: 0,
    }];
    let msg = Message {
        kind: Command::Block(Block {
            block_header,
            proof,
            txns: txns,
        }),
        magic: 1,
    };

    let expected_buf: Vec<u8> = [
        8, 1, 18, 225, 7, 58, 222, 7, 10, 74, 18, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 34, 10, 32, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18,
        73, 10, 71, 10, 69, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 196, 6, 18, 72, 26, 70, 10, 34, 10, 32,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 26, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 18, 72, 18, 70, 10, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 38, 34, 36, 10, 34,
        10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 26, 24, 10, 22, 10, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 26, 222, 2, 18, 219, 2, 10, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 18, 194, 2, 18, 98, 18, 93, 104, 116, 116, 112, 115, 58, 47, 47, 111, 112, 101, 110,
        119, 101, 97, 116, 104, 101, 114, 109, 97, 112, 46, 111, 114, 103, 47, 100, 97, 116, 97,
        47, 50, 46, 53, 47, 119, 101, 97, 116, 104, 101, 114, 63, 105, 100, 61, 50, 57, 53, 48, 49,
        53, 57, 38, 97, 112, 112, 105, 100, 61, 98, 54, 57, 48, 55, 100, 50, 56, 57, 101, 49, 48,
        100, 55, 49, 52, 97, 54, 101, 56, 56, 98, 51, 48, 55, 54, 49, 102, 97, 101, 50, 50, 26, 1,
        0, 18, 98, 18, 93, 104, 116, 116, 112, 115, 58, 47, 47, 111, 112, 101, 110, 119, 101, 97,
        116, 104, 101, 114, 109, 97, 112, 46, 111, 114, 103, 47, 100, 97, 116, 97, 47, 50, 46, 53,
        47, 119, 101, 97, 116, 104, 101, 114, 63, 105, 100, 61, 50, 57, 53, 48, 49, 53, 57, 38, 97,
        112, 112, 105, 100, 61, 98, 54, 57, 48, 55, 100, 50, 56, 57, 101, 49, 48, 100, 55, 49, 52,
        97, 54, 101, 56, 56, 98, 51, 48, 55, 54, 49, 102, 97, 101, 50, 50, 26, 1, 0, 26, 3, 10, 1,
        0, 34, 3, 10, 1, 0, 42, 54, 18, 52, 104, 116, 116, 112, 115, 58, 47, 47, 104, 111, 111,
        107, 115, 46, 122, 97, 112, 105, 101, 114, 46, 99, 111, 109, 47, 104, 111, 111, 107, 115,
        47, 99, 97, 116, 99, 104, 47, 51, 56, 54, 48, 53, 52, 51, 47, 108, 50, 97, 119, 99, 100,
        47, 42, 54, 18, 52, 104, 116, 116, 112, 115, 58, 47, 47, 104, 111, 111, 107, 115, 46, 122,
        97, 112, 105, 101, 114, 46, 99, 111, 109, 47, 104, 111, 111, 107, 115, 47, 99, 97, 116, 99,
        104, 47, 51, 56, 54, 48, 53, 52, 51, 47, 108, 49, 97, 119, 99, 119, 47, 26, 38, 26, 36, 10,
        34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 26, 58, 34, 56, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 26, 58, 42, 56, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 107, 10, 71, 10, 69, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 33, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 32,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0,
    ]
    .to_vec();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();
    assert_eq!(result, expected_buf);
}

#[test]
fn message_block_from_bytes() {
    let buf: Vec<u8> = [
        8, 1, 18, 225, 7, 58, 222, 7, 10, 74, 18, 36, 18, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 34, 10, 32, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18,
        73, 10, 71, 10, 69, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 196, 6, 18, 72, 26, 70, 10, 34, 10, 32,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 26, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 18, 72, 18, 70, 10, 34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 38, 34, 36, 10, 34,
        10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 26, 24, 10, 22, 10, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 26, 222, 2, 18, 219, 2, 10, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 18, 194, 2, 18, 98, 18, 93, 104, 116, 116, 112, 115, 58, 47, 47, 111, 112, 101, 110,
        119, 101, 97, 116, 104, 101, 114, 109, 97, 112, 46, 111, 114, 103, 47, 100, 97, 116, 97,
        47, 50, 46, 53, 47, 119, 101, 97, 116, 104, 101, 114, 63, 105, 100, 61, 50, 57, 53, 48, 49,
        53, 57, 38, 97, 112, 112, 105, 100, 61, 98, 54, 57, 48, 55, 100, 50, 56, 57, 101, 49, 48,
        100, 55, 49, 52, 97, 54, 101, 56, 56, 98, 51, 48, 55, 54, 49, 102, 97, 101, 50, 50, 26, 1,
        0, 18, 98, 18, 93, 104, 116, 116, 112, 115, 58, 47, 47, 111, 112, 101, 110, 119, 101, 97,
        116, 104, 101, 114, 109, 97, 112, 46, 111, 114, 103, 47, 100, 97, 116, 97, 47, 50, 46, 53,
        47, 119, 101, 97, 116, 104, 101, 114, 63, 105, 100, 61, 50, 57, 53, 48, 49, 53, 57, 38, 97,
        112, 112, 105, 100, 61, 98, 54, 57, 48, 55, 100, 50, 56, 57, 101, 49, 48, 100, 55, 49, 52,
        97, 54, 101, 56, 56, 98, 51, 48, 55, 54, 49, 102, 97, 101, 50, 50, 26, 1, 0, 26, 3, 10, 1,
        0, 34, 3, 10, 1, 0, 42, 54, 18, 52, 104, 116, 116, 112, 115, 58, 47, 47, 104, 111, 111,
        107, 115, 46, 122, 97, 112, 105, 101, 114, 46, 99, 111, 109, 47, 104, 111, 111, 107, 115,
        47, 99, 97, 116, 99, 104, 47, 51, 56, 54, 48, 53, 52, 51, 47, 108, 50, 97, 119, 99, 100,
        47, 42, 54, 18, 52, 104, 116, 116, 112, 115, 58, 47, 47, 104, 111, 111, 107, 115, 46, 122,
        97, 112, 105, 101, 114, 46, 99, 111, 109, 47, 104, 111, 111, 107, 115, 47, 99, 97, 116, 99,
        104, 47, 51, 56, 54, 48, 53, 52, 51, 47, 108, 49, 97, 119, 99, 119, 47, 26, 38, 26, 36, 10,
        34, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 26, 58, 34, 56, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 26, 58, 42, 56, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 107, 10, 71, 10, 69, 10, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 33, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 32,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0,
    ]
    .to_vec();

    let block_header = BlockHeader {
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
    let proof = LeadershipProof {
        block_sig: Some(signature.clone()),
        influence: 0,
    };
    let keyed_signature = vec![KeyedSignature {
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
        reveal: [0; 32].to_vec(),
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
    let inputs = vec![commit_input, data_request_input, reveal_input];
    let outputs = vec![
        value_transfer_output,
        data_request_output,
        commit_output,
        reveal_output,
        consensus_output,
    ];
    let txns: Vec<Transaction> = vec![Transaction {
        inputs,
        signatures: keyed_signature,
        outputs,
        version: 0,
    }];
    let expected_msg = Message {
        kind: Command::Block(Block {
            block_header,
            proof,
            txns: txns,
        }),
        magic: 1,
    };

    assert_eq!(Message::from_pb_bytes(&buf).unwrap(), expected_msg);
}

#[test]
fn message_block_encode_decode() {
    let block_header = BlockHeader {
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
    let proof = LeadershipProof {
        block_sig: Some(signature.clone()),
        influence: 0,
    };
    let keyed_signature = vec![KeyedSignature {
        public_key: [0; 32],
        signature,
    }];

    let commit_input = Input::Commit(CommitInput {
        nonce: 0,
        output_index: 0,
        reveal: [0; 32].to_vec(),
        transaction_id: Hash::SHA256([0; 32]),
    });
    let data_request_input = Input::DataRequest(DataRequestInput {
        output_index: 0,
        poe: [0; 32],
        transaction_id: Hash::SHA256([0; 32]),
    });
    let reveal_input = Input::Reveal(RevealInput {
        output_index: 0,
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
    let inputs = vec![data_request_input, reveal_input, commit_input];
    let outputs = vec![
        value_transfer_output,
        data_request_output,
        commit_output,
        reveal_output,
        consensus_output,
    ];
    let txns: Vec<Transaction> = vec![Transaction {
        inputs,
        signatures: keyed_signature,
        outputs,
        version: 0,
    }];
    let msg = Message {
        kind: Command::Block(Block {
            block_header,
            proof,
            txns,
        }),
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
    let signature = Signature::Secp256k1(Secp256k1Signature {
        r: [0; 32],
        s: [0; 32],
        v: 0,
    });
    let keyed_signature = vec![KeyedSignature {
        public_key: [0; 32],
        signature,
    }];

    let commit_input = Input::Commit(CommitInput {
        nonce: 0,
        output_index: 0,
        reveal: [0; 32].to_vec(),
        transaction_id: Hash::SHA256([0; 32]),
    });
    let data_request_input = Input::DataRequest(DataRequestInput {
        output_index: 0,
        poe: [0; 32],
        transaction_id: Hash::SHA256([0; 32]),
    });
    let reveal_input = Input::Reveal(RevealInput {
        output_index: 0,
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
        reveal: vec![],
        value: 0,
    });
    let consensus_output = Output::Tally(TallyOutput {
        pkh: [0; 20],
        result: vec![0; 1024], // The maximum size is not defined
        value: 0,
    });
    let inputs = vec![data_request_input, reveal_input, commit_input];
    let outputs = vec![
        value_transfer_output,
        data_request_output,
        commit_output,
        reveal_output,
        consensus_output,
    ];
    let msg = Message {
        kind: Command::Transaction(Transaction {
            version: 0,
            inputs,
            outputs,
            signatures: keyed_signature,
        }),
        magic: 1,
    };
    let cloned_msg = msg.clone();
    let result: Vec<u8> = msg.to_pb_bytes().unwrap();

    assert_eq!(cloned_msg, Message::from_pb_bytes(&result).unwrap());
}
