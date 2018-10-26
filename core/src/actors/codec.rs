use std::io;
use std::io::Cursor;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use actix::Message;
use bytes::BytesMut;
use log::info;
use tokio::codec::{Decoder, Encoder};

const HEADER_SIZE: u16 = 2; // bytes

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
#[derive(Debug, Message, Eq, PartialEq, Clone)]
pub struct P2PCodec;

/// Implement decoder trait for P2P codec
///
impl Decoder for P2PCodec {
    type Item = Request;
    type Error = io::Error;

    /// Method to decode bytes to a request
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut ftb: Option<Self::Item> = None;
        let msg_len = src.len() as u16;
        if msg_len > 2 {
            let mut header_vec = Cursor::new([src.to_vec()[0], src.to_vec()[1]]);
            let msg_size = header_vec.read_u16::<BigEndian>().unwrap();
            if msg_len >= (msg_size + HEADER_SIZE) {
                src.split_to(HEADER_SIZE as usize);
                ftb = Some(Request::Message(src.split_to(msg_size as usize)));
            }
        }
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
