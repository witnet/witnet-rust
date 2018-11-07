use bytes::BytesMut;
use tokio::codec::{Decoder, Encoder};
use witnet_core::actors::codec::{P2PCodec, Request, Response};

#[test]
fn core_actors_codec_p2p_decoder() {
    let mut buf: BytesMut = BytesMut::from(
        [
            0, 48, 16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 5, 12,
            0, 0, 0, 0, 0, 6, 0, 12, 0, 4, 0, 6, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0,
        ]
        .to_vec(),
    );
    let msg: BytesMut = BytesMut::from(
        [
            16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 5, 12, 0, 0,
            0, 0, 0, 6, 0, 12, 0, 4, 0, 6, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0,
        ]
        .to_vec(),
    );

    assert_eq!(
        Request::Message(msg),
        P2PCodec::decode(&mut P2PCodec {}, &mut buf)
            .unwrap()
            .unwrap()
    );
}

#[test]
fn core_actors_codec_p2p_encoder() {
    let decoded: BytesMut = BytesMut::from(
        [
            16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 5, 12, 0, 0,
            0, 0, 0, 6, 0, 12, 0, 4, 0, 6, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0,
        ]
        .to_vec(),
    );

    let encoded: BytesMut = BytesMut::from(
        [
            0, 48, 16, 0, 0, 0, 0, 0, 10, 0, 14, 0, 0, 0, 7, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0, 5, 12,
            0, 0, 0, 0, 0, 6, 0, 12, 0, 4, 0, 6, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0,
        ]
        .to_vec(),
    );

    let mut dst = BytesMut::with_capacity(1024);
    P2PCodec::encode(&mut P2PCodec {}, Response::Message(decoded), &mut dst).unwrap();
    assert_eq!(dst, encoded);
}
