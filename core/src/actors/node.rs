use std::io;
use std::path::PathBuf;
use std::process::exit;
use std::result::Result;

use actix::{Actor, System};
use log::info;

use crate::actors::blocks_manager::BlocksManager;
use crate::actors::config_manager::ConfigManager;
use crate::actors::connections_manager::ConnectionsManager;
use crate::actors::epoch_manager::EpochManager;
use crate::actors::peers_manager::PeersManager;
use crate::actors::sessions_manager::SessionsManager;
use crate::actors::storage_manager::StorageManager;

/// Function to run the main system
pub fn run(config: Option<PathBuf>, callback: fn()) -> Result<(), io::Error> {
    // Init system
    let system = System::new("node");

    // Call cb function (register interrupt handlers)
    callback();

    // Start config manager actor
    let config_manager_addr = ConfigManager::new(config).start();
    System::current().registry().set(config_manager_addr);

    // Start storage manager actor
    let storage_manager_addr = StorageManager::default().start();
    System::current().registry().set(storage_manager_addr);

    // Start peers manager actor
    let peers_manager_addr = PeersManager::default().start();
    System::current().registry().set(peers_manager_addr);

    // Start connections manager actor
    let connections_manager_addr = ConnectionsManager::default().start();
    System::current().registry().set(connections_manager_addr);

    // Start session manager actor
    let sessions_manager_addr = SessionsManager::default().start();
    System::current().registry().set(sessions_manager_addr);

    // Start epoch manager actor
    let epoch_manager_addr = EpochManager::default().start();
    System::current().registry().set(epoch_manager_addr);

    // Start blocks manager actor
    let blocks_manager_addr = BlocksManager::default().start();
    System::current().registry().set(blocks_manager_addr);

    // Run system
    system.run();

    Ok(())
}

/// Function to close the main system
pub fn close() {
    info!("Closing node");

    // FIXME(#72): find out how to gracefully stop the system
    // System::current().stop();

    // Process exit
    exit(0);
}
