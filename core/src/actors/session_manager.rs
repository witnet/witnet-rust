use std::collections::HashMap;

use actix::{Addr, Message, Actor, Context, Handler, SystemService};
use log::info;
use rand::Rng;

use crate::actors::session::{Session, SessionType};

/// Messages for session management

/// Message to indicate that a new session is created
pub struct Connect {
    /// Address of the session actor that is to be connected
    addr: Addr<Session>,

    /// Session type
    session_type: SessionType
}

/// Session manager returns unique session id for Connect message
impl Message for Connect {
    type Result = usize;
}

/// Helper functions
impl Connect {
    /// Method to create a Connect message
    pub fn new(addr: Addr<Session>, session_type: SessionType) -> Connect {
        Connect {addr, session_type}
    }
}

/// Message to indicate that a session is disconnected
#[derive(Message)]
pub struct Disconnect {
    /// Id of the session that is to be disconnected
    id: usize,

    /// Session type
    session_type: SessionType
}

impl Disconnect {
    /// Method to create a Disconnect message
    pub fn new(id: usize, session_type: SessionType) -> Disconnect {
        Disconnect { id, session_type }
    }
}

/// Session manager actor
#[derive(Default)]
pub struct SessionManager {
    /// Server sessions
    server_sessions: HashMap<usize, Addr<Session>>,

    /// Client sessions
    client_sessions: HashMap<usize, Addr<Session>>
}

impl SessionManager {
    /// Method to build a new session manager
    pub fn new() -> SessionManager {
        SessionManager::default()
    }

    /// Method to send a message through all client connections
    pub fn broadcast(&self, _message: &str, _skip_id: usize) {
    }
}

/// Make actor from `SessionManager`
impl Actor for SessionManager {
    /// Every actor has to provide execution `Context` in which it can run.
    type Context = Context<Self>;
}

/// Required traits for being able to retrieve session manager address from registry
impl actix::Supervised for SessionManager {}
impl SystemService for SessionManager {
    fn service_started(&mut self, _ctx: &mut Context<Self>) {
    }
}

/// Handler for Connect message.
impl Handler<Connect> for SessionManager {
    type Result = usize;

    fn handle(&mut self, msg: Connect, _: &mut Context<Self>) -> Self::Result {
        // Get random id to register session
        let id = rand::thread_rng().gen::<usize>();

        // Get map to insert session to
        let sessions = match msg.session_type {
            SessionType::Server => &mut self.server_sessions,
            SessionType::Client => &mut self.client_sessions
        };

        // Insert session in the right map
        sessions.insert(id, msg.addr);

        info!("Session {} registered", id);

        // Send id back
        id
    }
}

/// Handler for Disconnect message.
impl Handler<Disconnect> for SessionManager {
    type Result = ();

    fn handle(&mut self, msg: Disconnect, _: &mut Context<Self>) {
        // Get map to insert session to
        let sessions = match msg.session_type {
            SessionType::Server => &mut self.server_sessions,
            SessionType::Client => &mut self.client_sessions
        };

        // Remove session from map
        sessions.remove(&msg.id);

        info!("Session {} unregistered", msg.id);
    }
}