//! Websockets server implementation.
use std::io;
use std::net;

use actix_web::{actix::*, server, ws, App, Binary};
use jsonrpc_core as rpc;

mod actors;

use self::actors::{
    controller::Controller,
    notifications::{Notifications, Notify},
    ws::Ws,
};

/// Server configuration builder.
pub struct Server<F> {
    workers: Option<usize>,
    factory: F,
    addr: net::SocketAddr,
}

impl<F> Server<F>
where
    F: Fn() -> rpc::IoHandler + Clone + Send + Sync + 'static,
{
    /// Create a new websockets server.
    ///
    /// Accepts a factory function that will be used to create the websocket request-handlers. The
    /// function receives a notification function that can be used by the handlers to send notifications
    /// to all opened sockets. This function will block the current thread until the server is stopped.
    /// The server can be configured with a builder-like pattern.
    pub fn new(factory: F) -> Self {
        let addr = net::SocketAddr::V4(net::SocketAddrV4::new(
            net::Ipv4Addr::new(127, 0, 0, 1),
            3200,
        ));
        let workers = Default::default();

        Self {
            factory,
            addr,
            workers,
        }
    }

    /// Set how many worker threads the server will have.
    pub fn workers(mut self, workers: Option<usize>) -> Self {
        self.workers = workers;
        self
    }

    /// Set the socket address to bind to.
    pub fn addr(mut self, addr: net::SocketAddr) -> Self {
        self.addr = addr;
        self
    }

    /// Starts a JsonRPC Websockets server.
    ///
    /// The factory is used to create handlers for the json-rpc messages sent to the server.
    pub fn start(self) -> io::Result<Addr<actix_net::server::Server>> {
        let Server {
            factory,
            workers,
            addr,
        } = self;

        let mut s = server::new(move || {
            // Ensure that the controller starts if no actor has started it yet. It will register with
            // `ProcessSignals` shut down even if no actors have subscribed. If we remove this line, the
            // controller will not be instanciated and our system will not listen for signals.
            Controller::from_registry();

            let f = factory.clone();

            App::new().resource("/", move |r| {
                r.f(move |req| {
                    let remote = req
                        .connection_info()
                        .remote()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "[unknown ip]".to_string());
                    let actor = Ws::new(remote, f());
                    ws::start(req, actor)
                })
            })
        })
        .bind(addr)?;

        if let Some(workers) = workers {
            s = s.workers(workers);
        }

        Ok(s.start())
    }
}

/// Notify all websocket connections.
pub fn notify(payload: Binary) {
    let n = Notifications::from_registry();
    n.do_send(Notify { payload });
}
