use std::{
    fmt::{Debug, Display},
    marker::Send,
};

use actix::{
    io::FramedWrite, Actor, ActorFuture, Context, ContextFutureSpawner, Handler, Message,
    StreamHandler, System, WrapFuture,
};
use log::{debug, error, warn};
use tokio::{codec::FramedRead, io::AsyncRead};

use super::SessionsManager;
use crate::actors::{
    codec::P2PCodec,
    messages::{
        AddPeers, Anycast, Broadcast, Consolidate, Create, Register, SessionsUnitResult, Unregister,
    },
    peers_manager::PeersManager,
    session::Session,
};

/// Handler for Create message.
impl Handler<Create> for SessionsManager {
    type Result = ();

    fn handle(&mut self, msg: Create, _ctx: &mut Context<Self>) {
        // Get handshake timeout
        let handshake_timeout = self.sessions.handshake_timeout;

        // Get server address
        let server_addr = self.sessions.server_address;

        // Get magic number
        let magic_number = self.sessions.magic_number;

        // Create a Session actor
        Session::create(move |ctx| {
            // Get server address (if not present, send local address instead)
            let server_addr = server_addr.unwrap_or_else(|| msg.stream.local_addr().unwrap());

            // Get remote peer address
            let remote_addr = msg.stream.peer_addr().unwrap();

            // Split TCP stream into read and write parts
            let (r, w) = msg.stream.split();

            // Add stream in session actor from the read part of the tcp stream
            Session::add_stream(FramedRead::new(r, P2PCodec), ctx);

            // Create the session actor and store in its state the write part of the tcp stream
            Session::new(
                server_addr,
                remote_addr,
                msg.session_type,
                FramedWrite::new(w, P2PCodec, ctx),
                handshake_timeout,
                magic_number,
            )
        });
    }
}

/// Handler for Register message.
impl Handler<Register> for SessionsManager {
    type Result = SessionsUnitResult;

    fn handle(&mut self, msg: Register, _: &mut Context<Self>) -> Self::Result {
        // Call method register session from sessions library
        let result = self
            .sessions
            .register_session(msg.session_type, msg.address, msg.actor);

        match &result {
            Ok(_) => debug!(
                "Session (type {:?}) registered for peer {}",
                msg.session_type, msg.address
            ),
            Err(error) => error!(
                "Error while registering peer {} (session type {:?}): {}",
                msg.address, msg.session_type, error
            ),
        }

        result
    }
}

/// Handler for Unregister message.
impl Handler<Unregister> for SessionsManager {
    type Result = SessionsUnitResult;

    fn handle(&mut self, msg: Unregister, _: &mut Context<Self>) -> Self::Result {
        // Call method register session from sessions library
        let result = self
            .sessions
            .unregister_session(msg.session_type, msg.status, msg.address);

        match &result {
            Ok(_) => debug!(
                "Session (type {:?}) unregistered for peer {}",
                msg.session_type, msg.address
            ),
            Err(error) => error!(
                "Error while unregistering peer {} (session type {:?}): {}",
                msg.address, msg.session_type, error
            ),
        }

        result
    }
}

/// Handler for Consolidate message.
impl Handler<Consolidate> for SessionsManager {
    type Result = SessionsUnitResult;

    fn handle(&mut self, msg: Consolidate, _: &mut Context<Self>) -> Self::Result {
        // Call method register session from sessions library
        let result = self
            .sessions
            .consolidate_session(msg.session_type, msg.address);

        // Get peers manager address
        let peers_manager_addr = System::current().registry().get::<PeersManager>();

        // Send AddPeers message to the peers manager
        // If the session is outbound, this won't give any new information (except the timestamp
        // being updated)
        // If the session is inbound, this might be a valid information to get a new potential peer
        peers_manager_addr.do_send(AddPeers {
            addresses: vec![msg.potential_new_peer],
        });

        match &result {
            Ok(_) => debug!(
                "Established a consolidated {:?} session with the peer at {}",
                msg.session_type, msg.address
            ),
            Err(error) => error!(
                "Error while consolidating {:?} session with the peer at {}: {:?}",
                msg.session_type, msg.address, error
            ),
        }

        result
    }
}

/// Handler for Anycast message
impl<T: 'static> Handler<Anycast<T>> for SessionsManager
where
    T: Message + Send + Debug + Display,
    T::Result: Send,
    Session: Handler<T>,
{
    type Result = ();

    fn handle(&mut self, msg: Anycast<T>, ctx: &mut Context<Self>) {
        debug!(
            "An Anycast<{}> message is now being forwarded to a random session",
            msg.command
        );

        // Request a random consolidated outbound session
        self.sessions
            .get_random_anycast_session()
            .map(|session_addr| {
                // Send message to session and await for response
                session_addr
                    // Send SendMessage message to session actor
                    // This returns a Request Future, representing an asynchronous message sending process
                    .send(msg.command)
                    // Convert a normal future into an ActorFuture
                    .into_actor(self)
                    // Process the response from the session
                    // This returns a FutureResult containing the socket address if present
                    .then(|res, act, _ctx| {
                        // Process the response from session
                        act.process_command_response(&res)
                    })
                    .wait(ctx);
            })
            .unwrap_or_else(|| {
                warn!("No consolidated outbound session was found");
            });
    }
}

/// Handler for Broadcast message
impl<T: 'static> Handler<Broadcast<T>> for SessionsManager
where
    T: Clone + Message + Send + Display,
    T::Result: Send,
    Session: Handler<T>,
{
    type Result = ();

    fn handle(&mut self, msg: Broadcast<T>, _ctx: &mut Context<Self>) {
        debug!(
            "A Broadcast<{}> message is now being forwarded to all sessions",
            msg.command
        );

        self.sessions
            .get_all_consolidated_sessions()
            .for_each(|session_addr| {
                // Send message to session and ignore errors
                session_addr.do_send(msg.command.clone());
            });
    }
}
