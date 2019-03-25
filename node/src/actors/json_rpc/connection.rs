use actix::{
    io::FramedWrite, io::WriteHandler, Actor, ActorFuture, Addr, AsyncContext, Context,
    ContextFutureSpawner, Running, StreamHandler, WrapFuture,
};
use tokio::{io::WriteHalf, net::TcpStream};

use bytes;
use bytes::BytesMut;
use log::*;
use std::{io, rc::Rc};

use super::{
    newline_codec::NewLineCodec,
    server::{JsonRpcServer, Unregister},
};
use jsonrpc_pubsub::{PubSubHandler, Session};
use std::sync::Arc;

/// A single JSON-RPC connection
pub struct JsonRpc {
    /// Stream
    pub framed: FramedWrite<WriteHalf<TcpStream>, NewLineCodec>,
    /// Reference to parent
    // Needed to send the `Unregister` message when the connection closes
    pub parent: Addr<JsonRpcServer>,
    /// IoHandler
    pub jsonrpc_io: Rc<PubSubHandler<Arc<Session>>>,
    /// Sender
    pub session: Arc<Session>,
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
    fn handle(&mut self, bytes: BytesMut, ctx: &mut Self::Context) {
        debug!("Got JSON-RPC message");
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

        let session = Arc::clone(&self.session);

        // Handle response asynchronously
        self.jsonrpc_io
            .handle_request(&msg, session)
            .into_actor(self)
            .then(|res, act, _ctx| {
                if let Ok(Some(response)) = res {
                    act.framed.write(BytesMut::from(response));
                }

                actix::fut::ok(())
            })
            .wait(ctx);
    }
}

impl StreamHandler<String, ()> for JsonRpc {
    fn handle(&mut self, item: String, _ctx: &mut Self::Context) {
        self.framed.write(BytesMut::from(item));
    }
}
