use witnet_data_structures::flatbuffers::protocol_generated::protocol::{
    get_root_as_message, Command, Message, MessageArgs, Ping, PingArgs,
};

#[test]
fn data_structures_message_ping_encode() {
    // Build up a serialized buffer algorithmically (initial capacity of 1024B)
    let mut builder = flatbuffers::FlatBufferBuilder::new_with_capacity(1024);

    // Sample command: ping with nonce set to 7
    let ping_command = Ping::create(&mut builder, &PingArgs { nonce: 7 });

    // Create sample message with magic number set to 0
    let message = Message::create(
        &mut builder,
        &MessageArgs {
            magic: 0,
            command_type: Command::Ping,
            command: Some(ping_command.as_union_value()),
        },
    );

    // Serialize the root of the object, without providing a file identifier.
    builder.finish(message, None);

    // Check flatbuffer data of type `&[u8]`
    let buf = builder.finished_data();

    let expected_buf: [u8; 48] = [
        16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 5, 12, 0, 0, 0, 0,
        0, 6, 0, 12, 0, 4, 0, 6, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0,
    ];
    assert_eq!(
        buf.len(),
        expected_buf.len(),
        "Arrays don't have the same length"
    );
    assert!(
        buf.iter().zip(expected_buf.iter()).all(|(a, b)| a == b),
        "Arrays are not equal"
    );
}

#[test]
fn data_structures_message_ping_decode() {
    // Access flatbuffer as if we had just received it.
    let buf = [
        16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 5, 12, 0, 0, 0, 0,
        0, 6, 0, 12, 0, 4, 0, 6, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0,
    ];

    // Get access to the root
    let message = get_root_as_message(&buf);

    // Check magic number
    assert_eq!(message.magic(), 0);

    // Get and test the `Command` union (`command` field).
    assert_eq!(message.command_type(), Command::Ping);

    // Get command and check nonce value
    let ping = message.command_as_ping().unwrap();
    let ping_nonce = ping.nonce();

    assert_eq!(ping_nonce, 7);
}
