use actix::{
    actors::resolver::{ConnectAddr, Resolver},
    ActorFuture, ContextFutureSpawner, Handler, SystemService, WrapFuture,
};

use witnet_p2p::sessions::SessionType;

use super::ConnectionsManager;
use crate::actors::messages::{InboundTcpConnect, OutboundTcpConnect};

/// Handler for InboundTcpConnect messages (built from inbound connections)
impl Handler<InboundTcpConnect> for ConnectionsManager {
    /// Response for message, which is defined by `ResponseType` trait
    type Result = ();

    /// Method to handle the InboundTcpConnect message
    fn handle(&mut self, msg: InboundTcpConnect, _ctx: &mut Self::Context) {
        // Request the creation of a new session actor from connection
        ConnectionsManager::request_session_creation(msg.stream, SessionType::Inbound);
    }
}

/// Handler for OutboundTcpConnect messages (requested for creating outgoing connections)
impl Handler<OutboundTcpConnect> for ConnectionsManager {
    /// Response for message, which is defined by `ResponseType` trait
    type Result = ();

    /// Method to handle the OutboundTcpConnect message
    fn handle(&mut self, msg: OutboundTcpConnect, ctx: &mut Self::Context) {
        // Get resolver from registry and send a ConnectAddr message to it
        Resolver::from_registry()
            .send(ConnectAddr(msg.address))
            .into_actor(self)
            .then(move |res, _act, _ctx| {
                ConnectionsManager::process_connect_addr_response(res, msg.feeler)
            })
            .wait(ctx);
    }
}
