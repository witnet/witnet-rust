use actix::{
    io::FramedWrite, io::WriteHandler, Actor, ActorFuture, Addr, AsyncContext, Context,
    ContextFutureSpawner, Running, StreamHandler, WrapFuture,
};

use bytes::BytesMut;
use std::{io, rc::Rc};

use super::{
    newline_codec::NewLineCodec,
    server::{JsonRpcServer, Unregister},
};
use futures_util::compat::Compat01As03;
use jsonrpc_pubsub::{PubSubHandler, Session};
use std::sync::Arc;

/// A single JSON-RPC connection
pub struct JsonRpc {
    /// Stream
    pub framed: FramedWrite<BytesMut, tokio::net::tcp::OwnedWriteHalf, NewLineCodec>,
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
impl StreamHandler<Result<BytesMut, io::Error>> for JsonRpc {
    /// This is main event loop for client requests
    fn handle(&mut self, result: Result<BytesMut, io::Error>, ctx: &mut Self::Context) {
        if result.is_err() {
            // TODO: how to handle this error?
            return;
        }
        let bytes = result.unwrap();
        log::debug!("Got JSON-RPC message");
        let msg = match std::str::from_utf8(&bytes) {
            Ok(msg) => {
                // A valid utf8 string is forwarded to the JSON-RPC parser
                // The message is assumed to be a valid JSON-RPC, otherwise an
                // error is returned through the socket.
                // For example, an empty string results in a JSON-RPC ParseError (-32700).
                log::debug!("{}", msg);
                msg
            }
            Err(e) => {
                // When the input is not a valid utf8 string, a
                // ParseError (-32700) is returned thought the socket
                // and the message is printed in the debug logs for further inspection.
                log::error!("Invalid UTF8 in JSON-RPC input");
                log::debug!("{:?}", e);

                // Generate a ParseError later by trying to parse an empty string
                ""
            }
        };

        let session = Arc::clone(&self.session);

        // Handle response asynchronously
        let fut01 = self.jsonrpc_io.handle_request(&msg, session);

        Compat01As03::new(fut01)
            .into_actor(self)
            .map(|res, act, _ctx| {
                if let Ok(Some(response)) = res {
                    act.framed.write(BytesMut::from(response.as_str()));
                }
            })
            .wait(ctx);
    }
}

impl StreamHandler<Result<String, ()>> for JsonRpc {
    fn handle(&mut self, result: Result<String, ()>, _ctx: &mut Self::Context) {
        if result.is_err() {
            // TODO: how to handle this error?
            return;
        }

        let item = result.unwrap();
        self.framed.write(BytesMut::from(item.as_str()));
    }
}
