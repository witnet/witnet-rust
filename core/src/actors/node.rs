use std::io;
use std::net::SocketAddr;
use std::process::exit;

use actix::{Actor, System};
use log::info;

use crate::actors::server::Server;
use crate::actors::client::Client;
use crate::actors::session_manager::SessionManager;

/// Function to run the main system
pub fn run(address: SocketAddr, callback: fn()) -> io::Result<()> {
    info!("Witnet server listening on {}", address);

    // Init system
    let system = System::new("node");

    // Call cb function (register interrupt handlers)
    callback();

    // Start session manager actor
    SessionManager::new().start();

    // Start server actor
    Server::new(address).start();

    // Start client actor
    let peer_addr = "127.0.0.1:11337".parse().unwrap();
    Client::new(peer_addr).start();

    // Run system
    system.run();

    Ok(())
}

/// Function to close the main system
pub fn close() {
    info!("Closing node");

    // TODO: Investigate how to stop gracefully the system
    // System::current().stop();

    // Process exit
    exit(0);
}
