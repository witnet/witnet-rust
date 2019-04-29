//! Defines an actor that handles a Websocket connection.
//!
//! See the [`Ws`](Ws) struct for more information.
use std::time::{Duration, Instant};

use actix_web::{actix::*, ws};
use jsonrpc_core as rpc;

use super::notifications as notify;

/// Actor to handle a Websocket connection.
pub struct Ws {
    /// Client must send ping at least once per 10 seconds
    /// (CLIENT_TIMEOUT), otherwise we drop connection
    last_heartbeat: Instant,

    /// JsonRPC handler in charge of handling the requests received through websockets.
    pub jsonrpc_handler: rpc::IoHandler,
}

impl Ws {
    /// Create a new [`Ws`](Ws) instance with the given JsonRPC handler factory.
    ///
    /// The handler factory will invoked during the actor startup... TODO: continue
    pub fn new(jsonrpc_handler: rpc::IoHandler) -> Self {
        Self {
            last_heartbeat: Instant::now(),
            jsonrpc_handler,
        }
    }
}

impl Actor for Ws {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        // Subscribe to Notify messages
        notify::Notifications::from_registry()
            .do_send(notify::Subscribe(ctx.address().recipient()));

        // Websockets heart beat handling
        ctx.run_interval(Duration::from_secs(5), |act, ctx| {
            let inactive_time = Instant::now().duration_since(act.last_heartbeat);
            let timeout = Duration::from_secs(10);

            if inactive_time > timeout {
                log::debug!("Websocket client heartbeat failed, disconnecting!");
                ctx.stop();
            } else {
                ctx.ping("");
            }
        });
    }
}

impl Handler<notify::Notify> for Ws {
    type Result = <notify::Notify as Message>::Result;

    fn handle(&mut self, msg: notify::Notify, ctx: &mut Self::Context) {
        ctx.text(msg.payload);
    }
}

impl StreamHandler<ws::Message, ws::ProtocolError> for Ws {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        self.last_heartbeat = Instant::now();
        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Pong(_) => {}
            ws::Message::Text(req) => {
                self.jsonrpc_handler
                    .handle_request(req.as_ref())
                    .into_actor(self)
                    .and_then(|resp_opt, _act, ctx| {
                        if let Some(resp) = resp_opt {
                            ctx.text(resp);
                        }
                        fut::ok(())
                    })
                    .spawn(ctx);
            }
            ws::Message::Binary(_) => {}
            ws::Message::Close(_) => {
                ctx.stop();
            }
        }
    }
}
