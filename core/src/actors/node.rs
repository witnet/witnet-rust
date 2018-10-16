use std::io;
use std::net::SocketAddr;
use std::process::exit;

use actix::{Actor, System};
use log::info;

use crate::actors::server::Server;
use crate::actors::client::Client;
use crate::actors::storage_manager::StorageManager;
use crate::actors::session_manager::SessionManager;

/// Function to run the main system
pub fn run(address: SocketAddr, db_root: String, callback: fn()) -> io::Result<()> {
    info!("Witnet server listening on {}", address);

    // Init system
    let system = System::new("node");

    // Call cb function (register interrupt handlers)
    callback();

    // Start storage manager actor
    let storage_manager_addr = StorageManager::new(&db_root).start();
    System::current().registry().set(storage_manager_addr);

    // Start session manager actor
    let session_manager_addr = SessionManager::new().start();
    System::current().registry().set(session_manager_addr);

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
