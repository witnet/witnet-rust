use actix::{fut::FutureResult, Actor, AsyncContext, MailboxError, System, SystemService};
use futures::Stream;
use log::{debug, info};
use tokio::net::{TcpListener, TcpStream};

use crate::actors::{
    config_manager::send_get_config_request,
    sessions_manager::{messages::Create, SessionsManager},
};

use witnet_config::config::Config;
use witnet_p2p::sessions::SessionType;

mod actor;
mod handlers;
/// Messages to hold the TCP stream from an inbound TCP connection
pub mod messages;

/// Connections manager actor
#[derive(Default)]
pub struct ConnectionsManager;

/// Required trait for being able to retrieve connections manager address from system registry
impl actix::Supervised for ConnectionsManager {}

/// Required trait for being able to retrieve connections manager address from system registry
impl SystemService for ConnectionsManager {}

/// Auxiliary methods for ConnectionsManager actor
impl ConnectionsManager {
    /// Method to start a server
    fn start_server(&mut self, ctx: &mut <Self as Actor>::Context) {
        debug!("Trying to start P2P server...");

        // Send message to ConfigManager and process response
        send_get_config_request(self, ctx, ConnectionsManager::process_config);
    }

    /// Method to request the creation of a session actor from a TCP stream
    fn request_session_creation(stream: TcpStream, session_type: SessionType) {
        // Get sessions manager address
        let sessions_manager_addr = System::current().registry().get::<SessionsManager>();

        // Send a message to SessionsManager to request the creation of a session
        sessions_manager_addr.do_send(Create {
            stream,
            session_type,
        });
    }

    /// Method to process resolver ConnectAddr response
    fn process_connect_addr_response(
        response: Result<messages::ResolverResult, MailboxError>,
    ) -> FutureResult<(), (), Self> {
        // Process the Result<ResolverResult, MailboxError>
        match response {
            Err(e) => {
                debug!("Unsuccessful communication with resolver: {}", e);
                actix::fut::err(())
            }
            Ok(res) => {
                // Process the ResolverResult
                match res {
                    Err(e) => {
                        debug!("Error while trying to connect to the peer: {}", e);
                        actix::fut::err(())
                    }
                    Ok(stream) => {
                        debug!("Connected to peer {:?}", stream.peer_addr());

                        // Request the creation of a new session actor from connection
                        ConnectionsManager::request_session_creation(stream, SessionType::Outbound);

                        actix::fut::ok(())
                    }
                }
            }
        }
    }

    /// Method to process the configuration received from the ConfigManager
    fn process_config(&mut self, ctx: &mut <Self as Actor>::Context, config: &Config) {
        // Bind TCP listener to this address
        // FIXME(#72): decide what to do with actor when server cannot be started
        let listener = TcpListener::bind(&config.connections.server_addr).unwrap();

        // Add message stream which will return a InboundTcpConnect for each incoming TCP connection
        ctx.add_message_stream(
            listener
                .incoming()
                .map_err(|_| ())
                .map(messages::InboundTcpConnect::new),
        );

        info!(
            "P2P server has been started at {:?}",
            &config.connections.server_addr
        );
    }
}
