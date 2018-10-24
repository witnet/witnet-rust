use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use actix::{Actor, Addr, AsyncContext, Context, Handler, Message, System, SystemService};
use log::{debug, info};

use crate::actors::connections_manager::{ConnectionsManager, OutboundTcpConnect};
use crate::actors::session::{Session, SessionType};

// TODO: Replace by query to Config Manager
const MAX_NUM_PEERS: usize = 8;

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////

type SessionsMap = HashMap<SocketAddr, SessionInfo>;

/// Session Status (used for bootstrapping)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SessionStatus {
    /// Recently created session (no handshake yet)
    Unconsolidated,
    /// Session with successful handshake
    Consolidated,
}

/// Session info
pub struct SessionInfo {
    /// Session Actor address
    pub actor: Addr<Session>,
    /// Session status
    pub status: SessionStatus,
}

/// Sessions manager actor
#[derive(Default)]
pub struct SessionsManager {
    /// Inbound sessions: __untrusted__ peers that connect to the server
    inbound_sessions: SessionsMap,

    /// Outbound sessions: __known__ peers that the node is connected
    outbound_sessions: SessionsMap,
}

impl SessionsManager {
    /// Method to send a message through all client connections
    pub fn broadcast(&self, _message: &str, _skip_id: usize) {}

    /// Method to get the sessions map by a session type
    fn get_sessions_by_type(&mut self, session_type: SessionType) -> &mut SessionsMap {
        match session_type {
            SessionType::Inbound => &mut self.inbound_sessions,
            SessionType::Outbound => &mut self.outbound_sessions,
        }
    }

    /// Method to periodically check the number of client sessions
    fn bootstrap_peers(&self, ctx: &mut Context<Self>) {
        // Schedule the execution of the check to 5 seconds
        ctx.run_later(Duration::from_secs(5), |act, ctx| {
            // Get number of peers
            let num_peers = act.outbound_sessions.keys().len();
            debug!("Number of peers {}", num_peers);

            if num_peers < MAX_NUM_PEERS {
                // TODO: Include "create connection" message to ConnectionsManager (after rebase)
                info!("Send message to Connections Manager to create a new peer connection");

                let connections_manager_addr =
                    System::current().registry().get::<ConnectionsManager>();
                connections_manager_addr.do_send(OutboundTcpConnect);
            }

            // Reschedule the check of the
            act.bootstrap_peers(ctx);
        });
    }
}

/// Make actor from `SessionsManager`
impl Actor for SessionsManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Sessions Manager actor has been started!");
        // We'll start the check peers process on sessions manager start
        self.bootstrap_peers(ctx);
    }
}

/// Required traits for being able to retrieve sessions manager address from registry
impl actix::Supervised for SessionsManager {}
impl SystemService for SessionsManager {}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGES
////////////////////////////////////////////////////////////////////////////////////////

/// Message to indicate that a new session is created
#[derive(Message)]
pub struct Register {
    /// Socket address to identify the peer
    pub address: SocketAddr,

    /// Address of the session actor that is to be connected
    pub actor: Addr<Session>,

    /// Session type
    pub session_type: SessionType,
}

/// Message to indicate that a session is disconnected
#[derive(Message)]
pub struct Unregister {
    /// Socket address to identify the peer
    pub address: SocketAddr,

    /// Session type
    pub session_type: SessionType,
}

/// Message to indicate that a session is disconnected
#[derive(Message)]
pub struct Update {
    /// Socket address to identify the peer
    pub address: SocketAddr,

    /// Session type
    pub session_type: SessionType,

    /// Session status
    pub session_status: SessionStatus,
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////

/// Handler for Connect message.
impl Handler<Register> for SessionsManager {
    type Result = ();

    fn handle(&mut self, msg: Register, _: &mut Context<Self>) -> Self::Result {
        // Get map to insert session to
        let sessions = self.get_sessions_by_type(msg.session_type);

        // Insert session in the right map
        sessions.insert(
            msg.address,
            SessionInfo {
                actor: msg.actor,
                status: SessionStatus::Consolidated,
            },
        );

        info!(
            "Session (type {:?}) registered for peer {}",
            msg.session_type, msg.address
        );
    }
}

/// Handler for Disconnect message.
impl Handler<Unregister> for SessionsManager {
    type Result = ();

    fn handle(&mut self, msg: Unregister, _: &mut Context<Self>) {
        // Get map to insert session to
        let sessions = self.get_sessions_by_type(msg.session_type);

        // Remove session from map
        sessions.remove(&msg.address);

        info!(
            "Session (type {:?}) unregistered for peer {}",
            msg.session_type, msg.address
        );
    }
}

/// Handler for Connect message.
impl Handler<Update> for SessionsManager {
    type Result = ();

    fn handle(&mut self, msg: Update, _: &mut Context<Self>) -> Self::Result {
        // Get map to insert session to
        let sessions = self.get_sessions_by_type(msg.session_type);

        // Insert session in the right map
        if let Some(session) = sessions.get_mut(&msg.address) {
            session.status = msg.session_status;
        }

        info!(
            "Session status updated to {:?} for peer {}",
            msg.session_status, msg.address
        );
    }
}
