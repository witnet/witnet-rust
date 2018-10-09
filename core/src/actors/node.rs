
use std::io;
use std::net::SocketAddr;
use std::process::exit;

use actix::{Actor, System};
use log::info;

use crate::actors::server::Server;

/// Function to run the main system
pub fn run(address: SocketAddr, callback: fn()) -> io::Result<()> {
    info!("Witnet server listening on {}", address);

    // callback with

    // Init system
    let system = System::new("node");

    callback();

    // Init server actor
    let server = Server::new(address);

    // Start server actor
    let _addr = server.start();

    // // Init client actor
    // let peer_addr = "127.0.0.1:12345".parse().unwrap();
    // let client = Client::new(peer_addr);

    // // Start client actor
    // let _addr = client.start();

    // Run system
    system.run();

    Ok(())
}

// TODO: Investigate how to stop gracefully the system
/// Function to close the main system
pub fn close() {
    info!("Closing node");
    // System::current().stop();

    exit(0);
}
