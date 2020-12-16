use witnet_data_structures::chain::PublicKey;
use witnet_data_structures::{proto::ProtobufConvert, types, types::IpAddress};

#[test]
fn address_proto() {
    // Serialize
    let addressv4 = types::Address {
        ip: IpAddress::Ipv4 { ip: 0x1020_3040 },
        port: 21337,
    };
    let address_bytes = addressv4.to_pb_bytes().unwrap();

    // Deserialize
    let address2v4 = types::Address::from_pb_bytes(&address_bytes).unwrap();

    assert_eq!(addressv4, address2v4);

    let addressv6 = types::Address {
        ip: IpAddress::Ipv6 {
            ip0: 0x1020_3040,
            ip1: 0xabcd,
            ip2: 0x21,
            ip3: 0x1111_1111,
        },
        port: 21337,
    };
    let address_bytes = addressv6.to_pb_bytes().unwrap();

    let address2v6 = types::Address::from_pb_bytes(&address_bytes).unwrap();

    assert_eq!(addressv6, address2v6);
}

#[test]
fn public_key_proto() {
    // Serialize
    let test_public_key = PublicKey {
        compressed: 0x03,
        bytes: [0x4a; 32],
    };
    let pk_bytes = test_public_key.to_pb_bytes().unwrap();

    // Deserialize
    let deserialize_public_key = PublicKey::from_pb_bytes(&pk_bytes).unwrap();

    assert_eq!(test_public_key, deserialize_public_key);
}
