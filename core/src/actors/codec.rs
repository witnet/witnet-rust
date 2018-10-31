use std::io;
use std::io::Cursor;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use actix::Message;
use bytes::BytesMut;
use log::info;
use tokio::codec::{Decoder, Encoder};

const HEADER_SIZE: usize = 2; // bytes

/// Message coming from the network
#[derive(Debug, Message, Eq, PartialEq, Clone)]
pub enum Request {
    /// Request message
    Message(BytesMut),
}

/// Message going to the network
#[derive(Debug, Message, Eq, PartialEq, Clone)]
pub enum Response {
    /// Response message
    Message(BytesMut),
}

/// Codec for client -> server transport
///
/// Format:
/// ```norun
/// Message size: u16
/// Message: [u8; Message size]
/// ```
///
/// The message format is described in the file [schemas/protocol.fbs][protocol]
///
/// [protocol]: https://github.com/witnet/witnet-rust/blob/master/schemas/protocol.fbs
#[derive(Debug, Message, Eq, PartialEq, Clone)]
pub struct P2PCodec;

/// Implement decoder trait for P2P codec
impl Decoder for P2PCodec {
    type Item = Request;
    type Error = io::Error;

    /// Method to decode bytes to a request
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut ftb: Option<Self::Item> = None;
        let msg_len = src.len();
        if msg_len >= HEADER_SIZE {
            let mut header_vec = Cursor::new(&src[0..HEADER_SIZE]);
            let msg_size = header_vec.read_u16::<BigEndian>().unwrap() as usize;
            if msg_len >= msg_size + HEADER_SIZE {
                src.split_to(HEADER_SIZE);
                ftb = Some(Request::Message(src.split_to(msg_size)));
            }
        }
        // If the message is incomplete, return without consuming anything.
        // This method will be called again when more bytes arrive.

        Ok(ftb)
    }
}

/// Implement encoder trait for P2P codec
impl Encoder for P2PCodec {
    type Item = Response;
    type Error = io::Error;

    /// Method to encode a response into bytes
    fn encode(&mut self, msg: Response, dst: &mut BytesMut) -> Result<(), Self::Error> {
        info!("Encoding {:?}", msg);

        let Response::Message(bytes) = msg;

        let mut encoded_msg = vec![];

        if bytes.len() > u16::max_value() as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Message size {} bytes too big for u16", bytes.len()),
            ));
        }
        let header: u16 = bytes.len() as u16;
        // push header with msg len
        encoded_msg.write_u16::<BigEndian>(header).unwrap();
        // push message
        encoded_msg.append(&mut bytes.to_vec());
        // push message to destination
        dst.unsplit(BytesMut::from(encoded_msg));
        Ok(())
    }
}
