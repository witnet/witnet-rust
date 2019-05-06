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
pub struct Server<F, S = ()> {
    workers: Option<usize>,
    factory: F,
    addr: net::SocketAddr,
    state: S,
}

impl<F> Server<F, ()>
where
    F: Fn(HandlerContext<'_, ()>) -> rpc::IoHandler + Clone + Send + Sync + 'static,
{
    /// Create a new websockets server with () state.
    ///
    /// The server can be configured with a builder-like pattern.
    pub fn new(factory: F) -> Self {
        Self::with_state((), factory)
    }
}

impl<S, F> Server<F, S>
where
    S: Send + Clone + 'static,
    F: Fn(HandlerContext<'_, S>) -> rpc::IoHandler + Clone + Send + Sync + 'static,
{
    /// Create a new websockets server with S state.
    ///
    /// Accepts a factory function that will be used to create the websocket request-handlers. The
    /// function receives a notification function that can be used by the handlers to send notifications
    /// to all opened sockets. This function will block the current thread until the server is stopped.
    /// The server can be configured with a builder-like pattern.
    pub fn with_state(state: S, factory: F) -> Server<F, S> {
        let addr = net::SocketAddr::V4(net::SocketAddrV4::new(
            net::Ipv4Addr::new(127, 0, 0, 1),
            3200,
        ));
        Self {
            state,
            factory,
            addr,
            workers: None,
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
    pub fn run(self) -> io::Result<()> {
        let factory = self.factory;
        let workers = self.workers;
        let state = self.state;
        let addr = self.addr;

        let mut s = server::new(move || {
            // Ensure that the controller starts if no actor has started it yet. It will register with
            // `ProcessSignals` shut down even if no actors have subscribed. If we remove this line, the
            // controller will not be instanciated and our system will not listen for signals.
            Controller::from_registry();

            let fact = factory.clone();

            App::with_state(state.clone()).resource("/", move |r| {
                r.f(move |req| {
                    ws::start(
                        req,
                        Ws::new(fact(HandlerContext {
                            state: req.state(),
                            notify,
                        })),
                    )
                })
            })
        })
        .bind(addr)?;

        if let Some(workers) = workers {
            s = s.workers(workers);
        }

        s.run();

        Ok(())
    }
}

/// Function passed to a JsonRPC handler factory so the handlers are able to send notifications to
/// other clients.
fn notify(payload: Binary) {
    let n = Notifications::from_registry();
    n.do_send(Notify { payload });
}

/// TODO: doc
pub struct HandlerContext<'a, S> {
    /// doc
    pub state: &'a S,
    /// doc
    pub notify: fn(Binary),
}
