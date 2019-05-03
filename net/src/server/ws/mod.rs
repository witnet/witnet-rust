//! Websockets server implementation.
use std::net::SocketAddr;

use actix_web::Binary;
use jsonrpc_core as rpc;

mod actors;

use self::actors::controller::Controller;

/// Marker trait that represents the factory functions for JsonRpc-request-handlers factory.
pub trait HandlerFactory: Fn(fn(Binary)) -> rpc::IoHandler + Clone + Send + Sync + 'static {}

impl<T> HandlerFactory for T where
    T: Fn(fn(Binary)) -> rpc::IoHandler + Clone + Send + Sync + 'static
{
}

/// Runs a JsonRPC websockets server.
///
/// Accepts a factory function that will be used to create the websocket request-handlers. The
/// function receives a notification function that can be used by the handlers to send notifications
/// to all opened sockets. This function will block the current thread until the server is stopped.
pub fn run<F>(addr: SocketAddr, jsonrpc_handler_factory: F) -> std::io::Result<()>
where
    F: HandlerFactory,
{
    build().run(addr, jsonrpc_handler_factory)
}

/// Create a new ServerConfig builder instance.
pub fn build() -> ServerBuilder {
    ServerBuilder::default()
}

/// Server configuration builder.
#[derive(Default)]
pub struct ServerBuilder {
    workers: Option<usize>,
}

/// Server configuration.
pub struct ServerConfig<F> {
    pub(crate) workers: Option<usize>,
    pub(crate) addr: SocketAddr,
    pub(crate) handler_factory: F,
}

impl ServerBuilder {
    /// Set how many worker threads the server will have.
    pub fn workers(mut self, workers: Option<usize>) -> Self {
        self.workers = workers;
        self
    }

    /// Start the server with the given request handler factory.
    pub fn run<F>(self, addr: SocketAddr, jsonrpc_handler_factory: F) -> std::io::Result<()>
    where
        F: HandlerFactory,
    {
        let config = ServerConfig {
            workers: self.workers,
            addr,
            handler_factory: jsonrpc_handler_factory,
        };
        Controller::run(config)
    }
}
