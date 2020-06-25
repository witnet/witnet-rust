use actix::prelude::*;
use futures::Stream;
use tokio::net::{TcpListener, TcpStream};

use crate::actors::{
    messages::{Create, InboundTcpConnect, ResolverResult},
    sessions_manager::SessionsManager,
};

use crate::config_mngr;

use crate::actors::peers_manager::PeersManager;
use std::net::SocketAddr;
use witnet_p2p::sessions::SessionType;

mod actor;
mod handlers;

/// Connections manager actor
#[derive(Default)]
pub struct ConnectionsManager;

/// Required trait for being able to retrieve connections manager address from system registry
impl actix::Supervised for ConnectionsManager {}

/// Required trait for being able to retrieve connections manager address from system registry
impl SystemService for ConnectionsManager {}

/// Auxiliary methods for ConnectionsManager actor
impl ConnectionsManager {
    fn start_server(&mut self, ctx: &mut <Self as Actor>::Context) {
        config_mngr::get()
            .into_actor(self)
            .and_then(|config, _, ctx| {
                // Bind TCP listener to this address
                // FIXME(#72): decide what to do with actor when server cannot be started
                let listener = TcpListener::bind(&config.connections.server_addr).unwrap();

                ctx.add_message_stream(
                    listener
                        .incoming()
                        .map_err(|err| {
                            log::error!("Error incoming listener: {}", err);
                        })
                        .map(InboundTcpConnect::new),
                );

                log::info!(
                    "P2P server has been started at {:?}",
                    &config.connections.server_addr
                );

                fut::ok(())
            })
            .map_err(|err, _, _| log::error!("P2P server failed to start: {}", err))
            .wait(ctx);
    }

    /// Method to request the creation of a session actor from a TCP stream
    fn request_session_creation(stream: TcpStream, session_type: SessionType) {
        // Get sessions manager address
        let sessions_manager_addr = SessionsManager::from_registry();

        // Send a message to SessionsManager to request the creation of a session
        sessions_manager_addr.do_send(Create {
            stream,
            session_type,
        });
    }

    /// Method to process resolver ConnectAddr response
    fn process_connect_addr_response(
        response: Result<ResolverResult, MailboxError>,
        session_type: SessionType,
        address: &SocketAddr,
    ) -> actix::fut::FutureResult<(), (), Self> {
        // Process the Result<ResolverResult, MailboxError>
        match response {
            Err(error) => {
                log::error!("Unsuccessful communication with resolver: {}", error);
                actix::fut::err(())
            }
            Ok(res) => {
                // Process the ResolverResult
                match res {
                    Err(error) => {
                        log::debug!(
                            "Failed to connect to peer address {} with error: {:?}",
                            address,
                            error
                        );
                        // Try to evict this address from `tried` buckets
                        PeersManager::remove_address_from_tried(address);

                        actix::fut::err(())
                    }
                    Ok(stream) => {
                        stream
                            .peer_addr()
                            .map(|ip| log::debug!("Connected to peer {:?}", ip))
                            .unwrap_or_else(|err| {
                                log::error!("Peer address error in stream: {}", err)
                            });

                        // Request the creation of a new session actor from connection
                        ConnectionsManager::request_session_creation(stream, session_type);

                        actix::fut::ok(())
                    }
                }
            }
        }
    }
}
