#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
#![allow(clippy::all)]
#![allow(missing_docs)]

/// # Example
///
/// The next lines present detailed usage example of Flatbuffers.
///
/// ```
/// use witnet_data_structures::flatbuffers::protocol_generated::protocol::{
///    get_root_as_message, Command, Message, MessageArgs, Ping, PingArgs,
/// };
///
/// // Build up a serialized buffer algorithmically
/// let mut builder = flatbuffers::FlatBufferBuilder::new_with_capacity(1024);
///
/// // Sample command: ping with nonce set to 7
/// let ping_command = Ping::create(&mut builder, &PingArgs { nonce: 7 });
///
/// // Create sample message with magic number set to 0
/// let message = Message::create(
///        &mut builder,
///        &MessageArgs {
///            magic: 0,
///            command_type: Command::Ping,
///            command: Some(ping_command.as_union_value())
///        },
/// );
///
/// // Serialize the root of the object, without providing a file identifier.
/// builder.finish(message, None);
///
/// // We now have a FlatBuffer we can store on disk or send over a network.
/// // ** file/network code goes here :) **
///
/// let buf = builder.finished_data(); // Of type `&[u8]`
///
/// // Get access to the root:
/// let message = get_root_as_message(buf);
/// assert_eq!(message.magic(), 0);
/// assert_eq!(message.command_type(), Command::Ping);
///
/// // Get command and check nonce value
/// let ping = message.command_as_ping().unwrap();
/// let ping_nonce = ping.nonce();
/// assert_eq!(ping_nonce, 7);
/// ```
pub mod protocol_generated;
