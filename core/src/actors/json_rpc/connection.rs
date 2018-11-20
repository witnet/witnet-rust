use super::newline_codec::NewLineCodec;
use super::server::JsonRpcServer;
use super::server::Unregister;
use actix::{
    io::FramedWrite, io::WriteHandler, Actor, Addr, AsyncContext, Context, Running, StreamHandler,
};
use bytes;
use bytes::BytesMut;
use jsonrpc_core::IoHandler;
use log::*;
use std::io;
use std::rc::Rc;
use tokio::io::WriteHalf;
use tokio::net::TcpStream;

/// A single JSON-RPC connection
pub struct JsonRpc {
    /// Stream
    pub framed: FramedWrite<WriteHalf<TcpStream>, NewLineCodec>,
    /// Reference to parent
    // Needed to send the `Unregister` message when the connection closes
    pub parent: Addr<JsonRpcServer>,
    /// IoHandler
    pub jsonrpc_io: Rc<IoHandler<()>>,
}

impl Actor for JsonRpc {
    type Context = Context<Self>;

    /// Method to be executed when the actor is stopping
    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        // Unregister session from JsonRpcServer
        self.parent.do_send(Unregister {
            addr: ctx.address(),
        });

        Running::Stop
    }
}

impl WriteHandler<io::Error> for JsonRpc {}

/// Implement `StreamHandler` trait in order to use `Framed` with an actor
impl StreamHandler<BytesMut, io::Error> for JsonRpc {
    /// This is main event loop for client requests
    fn handle(&mut self, bytes: BytesMut, _ctx: &mut Self::Context) {
        info!("Got JSON-RPC message");
        let msg = match String::from_utf8(bytes.to_vec()) {
            Ok(msg) => {
                // A valid utf8 string is forwarded to the JSON-RPC parser
                // The message is assumed to be a valid JSON-RPC, otherwise an
                // error is returned through the socket.
                // For example, an empty string results in a JSON-RPC ParseError (-32700).
                debug!("{}", msg);
                msg
            }
            Err(e) => {
                // When the input is not a valid utf8 string, a
                // ParseError (-32700) is returned thought the socket
                // and the message is printed in the debug logs for further inspection.
                error!("Invalid UTF8 in JSON-RPC input");
                debug!("{:?}", e);

                // Generate a ParseError later by trying to parse an empty string
                "".to_string()
            }
        };

        // Handle response synchronously
        let response = self.jsonrpc_io.handle_request_sync(&msg);
        if let Some(response) = response {
            self.framed.write(BytesMut::from(response));
        }
    }
}
