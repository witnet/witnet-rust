use std::{path::PathBuf, process::exit, result::Result};

use actix::{Actor, System};
use failure;
use futures::future::Future;
use log::info;

use crate::actors::{
    chain_manager::ChainManager, connections_manager::ConnectionsManager,
    epoch_manager::EpochManager, inventory_manager::InventoryManager, json_rpc::JsonRpcServer,
    peers_manager::PeersManager, rad_manager::RadManager, sessions_manager::SessionsManager,
    storage_manager::StorageManager,
};
use crate::config_mngr;

/// Function to run the main system
pub fn run(
    config: Option<PathBuf>,
    fallback_config: Option<PathBuf>,
    callback: fn(),
) -> Result<(), failure::Error> {
    // Init system
    let system = System::new("node");

    // Call cb function (register interrupt handlers)
    callback();

    // Start ConfigManager actor
    config_mngr::start();
    match (config, fallback_config) {
        (Some(filename), _) => actix::Arbiter::spawn(
            config_mngr::load_from_file(filename).map_err(|_| System::current().stop()),
        ),
        (_, Some(filename)) => {
            actix::Arbiter::spawn(config_mngr::load_from_file(filename).map_err(|_| ()))
        }
        _ => (),
    }

    // Start StorageManager actor
    let storage_manager_addr = StorageManager::default().start();
    System::current().registry().set(storage_manager_addr);

    // Start PeersManager actor
    let peers_manager_addr = PeersManager::default().start();
    System::current().registry().set(peers_manager_addr);

    // Start ConnectionsManager actor
    let connections_manager_addr = ConnectionsManager::default().start();
    System::current().registry().set(connections_manager_addr);

    // Start SessionManager actor
    let sessions_manager_addr = SessionsManager::default().start();
    System::current().registry().set(sessions_manager_addr);

    // Start EpochManager actor
    let epoch_manager_addr = EpochManager::default().start();
    System::current().registry().set(epoch_manager_addr);

    // Start ChainManager actor
    let chain_manager_addr = ChainManager::default().start();
    System::current().registry().set(chain_manager_addr);

    // Start InventoryManager actor
    let inventory_manager_addr = InventoryManager::default().start();
    System::current().registry().set(inventory_manager_addr);

    // Start RadManager actor
    let rad_manager_addr = RadManager::default().start();
    System::current().registry().set(rad_manager_addr);

    // Start JSON RPC server (this doesn't need to be in the registry)
    let _json_rpc_server_addr = JsonRpcServer::default().start();

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
