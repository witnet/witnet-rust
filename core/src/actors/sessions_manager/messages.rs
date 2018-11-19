use std::{marker::Send, net::SocketAddr};

use actix::{Addr, Handler, Message};
use tokio::net::TcpStream;

use witnet_p2p::sessions::{error::SessionsResult, SessionStatus, SessionType};

use crate::actors::session::Session;

/// Message result of unit
pub type SessionsUnitResult = SessionsResult<()>;

/// Message indicating a new session needs to be created
pub struct Create {
    /// TCP stream
    pub stream: TcpStream,

    /// Session type
    pub session_type: SessionType,
}

impl Message for Create {
    type Result = ();
}

/// Message indicating a new session needs to be registered
pub struct Register {
    /// Socket address which identifies the peer
    pub address: SocketAddr,

    /// Address of the session actor that is to be connected
    pub actor: Addr<Session>,

    /// Session type
    pub session_type: SessionType,
}

impl Message for Register {
    type Result = SessionsUnitResult;
}

/// Message indicating a session needs to be unregistered
pub struct Unregister {
    /// Socket address identifying the peer
    pub address: SocketAddr,

    /// Session type
    pub session_type: SessionType,

    /// Session status
    pub status: SessionStatus,
}

impl Message for Unregister {
    type Result = SessionsUnitResult;
}

/// Message indicating a session needs to be consolidated
pub struct Consolidate {
    /// Socket address which identifies the peer
    pub address: SocketAddr,

    /// Potential peer to be added
    /// In their `Version` messages the nodes communicate the address of their server and that
    /// is a potential peer that should try to be added
    pub potential_new_peer: SocketAddr,

    /// Session type
    pub session_type: SessionType,
}

impl Message for Consolidate {
    type Result = SessionsUnitResult;
}

/// Message indicating a message is to be forwarded to a random consolidated outbound session
pub struct Anycast<T> {
    /// Command to be sent to the session
    pub command: T,
}

impl<T> Message for Anycast<T>
where
    T: Message + Send,
    T::Result: Send,
    Session: Handler<T>,
{
    type Result = ();
}

/// Message indicating a message is to be forwarded to all the consolidated outbound sessions
pub struct Broadcast<T> {
    /// Command to be sent to all the sessions
    pub command: T,
}

impl<T> Message for Broadcast<T>
where
    T: Clone + Message + Send,
    T::Result: Send,
    Session: Handler<T>,
{
    type Result = ();
}
