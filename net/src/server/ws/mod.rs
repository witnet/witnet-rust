//! Websockets server implementation.

use actix_web::Binary;
use jsonrpc_core as rpc;

mod actors;

use self::actors::controller::Controller;

/// Runs a JsonRPC websockets server.
///
/// Accepts a factory function that will be used to create the websocket request-handlers. The
/// function receives a notification function that can be used by the handlers to send notifications
/// to all opened sockets. This function will block the current thread until the server is stopped.
pub fn run<F>(jsonrpc_handler_factory: F) -> std::io::Result<()>
where
    F: Fn(fn(Binary)) -> rpc::IoHandler + Clone + Send + Sync + 'static,
{
    Controller::run(jsonrpc_handler_factory)
}
