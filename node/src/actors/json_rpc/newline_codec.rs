use actix::Message;
use bytes::{BufMut, BytesMut};
use std::io;
use tokio_util::codec::{Decoder, Encoder};

/// Codec for JSON-RPC transport
///
/// Read until the first newline (`\n`).
/// The newline is stripped from the returned message.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct NewLineCodec;

impl Message for NewLineCodec {
    type Result = ();
}

/// Implement decoder trait for NewLineCodec
impl Decoder for NewLineCodec {
    type Item = BytesMut;
    type Error = io::Error;

    /// Method to decode bytes to a request
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut ftb: Option<Self::Item> = None;

        let new_line_pos = src.iter().position(|&x| x == b'\n');
        if let Some(new_line_pos) = new_line_pos {
            // Split the message at the first newline
            let mut msg = src.split_to(new_line_pos + 1);
            // Strip that newline from the returned bytes
            msg.truncate(new_line_pos);
            ftb = Some(msg);
        }
        // If the message is incomplete, return without consuming anything.
        // This method will be called again when more bytes arrive.

        Ok(ftb)
    }
}

/// Implement encoder trait for NewLineCodec
impl Encoder<BytesMut> for NewLineCodec {
    type Error = io::Error;

    /// Method to encode a response into bytes. The input should not contain
    /// any newline characters, as the message will not be decoded correctly.
    fn encode(&mut self, bytes: BytesMut, dst: &mut BytesMut) -> Result<(), Self::Error> {
        //log::debug!("Encoding {:?}", bytes);
        // push message
        dst.put(bytes);
        // finish with a newline
        dst.put_u8(b'\n');
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let mut empty_buf = BytesMut::from(&[][..]);
        let mut c = NewLineCodec;
        assert_eq!(None, c.decode(&mut empty_buf).unwrap());
    }

    #[test]
    fn no_newline() {
        let mut input = BytesMut::from(&b"abcd"[..]);
        let original = input.clone();
        let mut c = NewLineCodec;
        // When there is no newline, the codec returns None
        assert_eq!(None, c.decode(&mut input).unwrap());
        // And the input is left unchanged
        assert_eq!(original, input);
    }

    #[test]
    fn only_newlines() {
        let mut empty_bytes = BytesMut::from(&b"\n\n\n\n"[..]);
        let expected = BytesMut::from(&[][..]);
        let mut c = NewLineCodec;
        // Exactly 4 newlines
        assert_eq!(Some(expected.clone()), c.decode(&mut empty_bytes).unwrap());
        assert_eq!(Some(expected.clone()), c.decode(&mut empty_bytes).unwrap());
        assert_eq!(Some(expected.clone()), c.decode(&mut empty_bytes).unwrap());
        // Now the buffer only contains one \n
        assert_eq!(BytesMut::from(&b"\n"[..]), empty_bytes);
        assert_eq!(Some(expected.clone()), c.decode(&mut empty_bytes).unwrap());
        // Now the buffer is empty
        assert_eq!(expected, empty_bytes);
        assert_eq!(None, c.decode(&mut empty_bytes).unwrap());
    }

    #[test]
    fn newlines_and_as() {
        let mut empty_bytes = BytesMut::from(&b"a\na\na\na\na"[..]);
        let expected = BytesMut::from(&b"a"[..]);
        let mut c = NewLineCodec;
        // Exactly 4 newlines
        assert_eq!(Some(expected.clone()), c.decode(&mut empty_bytes).unwrap());
        assert_eq!(Some(expected.clone()), c.decode(&mut empty_bytes).unwrap());
        assert_eq!(Some(expected.clone()), c.decode(&mut empty_bytes).unwrap());
        // Now the buffer only contains "a\na"
        assert_eq!(BytesMut::from(&b"a\na"[..]), empty_bytes);
        assert_eq!(Some(expected), c.decode(&mut empty_bytes).unwrap());
        // Now the buffer only contains an "a", with no newline,
        // so the codec will return None and wait for more bytes to arrive
        assert_eq!(BytesMut::from(&b"a"[..]), empty_bytes);
        assert_eq!(None, c.decode(&mut empty_bytes).unwrap());
        assert_eq!(BytesMut::from(&b"a"[..]), empty_bytes);
    }

    #[test]
    fn isomorphic() {
        let mut input = BytesMut::from(&b"A long string with some\n newlines.\n"[..]);
        let original = input.clone();
        let mut decoded = vec![];
        let mut c = NewLineCodec;

        // Decoding a message and encoding it again results in the original message
        while let Some(x) = c.decode(&mut input).unwrap() {
            decoded.push(x);
        }

        let mut buf = BytesMut::from(&[][..]);
        for d in decoded {
            c.encode(d, &mut buf).unwrap();
        }

        assert_eq!(original, buf);
    }
}
