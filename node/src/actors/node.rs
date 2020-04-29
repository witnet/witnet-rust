use std::{process::exit, result::Result};

use actix::{Actor, System, SystemRegistry};
use futures::future::Future;

use crate::actors::{
    chain_manager::ChainManager, connections_manager::ConnectionsManager,
    epoch_manager::EpochManager, inventory_manager::InventoryManager, json_rpc::JsonRpcServer,
    peers_manager::PeersManager, rad_manager::RadManager, sessions_manager::SessionsManager,
};
use crate::config_mngr;
use crate::signature_mngr;
use crate::storage_mngr;
use witnet_config::config::Config;

/// Function to run the main system
pub fn run(config: Config, callback: fn()) -> Result<(), failure::Error> {
    // Init system
    let system = System::new("node");

    // Call cb function (register interrupt handlers)
    callback();

    // Start ConfigManager actor
    config_mngr::start();
    actix::Arbiter::spawn(config_mngr::set(config).map_err(|_| System::current().stop()));

    // Start StorageManager actor & SignatureManager
    storage_mngr::start();
    signature_mngr::start();

    // Start PeersManager actor
    let peers_manager_addr = PeersManager::default().start();
    SystemRegistry::set(peers_manager_addr);

    // Start ConnectionsManager actor
    let connections_manager_addr = ConnectionsManager::default().start();
    SystemRegistry::set(connections_manager_addr);

    // Start SessionManager actor
    let sessions_manager_addr = SessionsManager::default().start();
    SystemRegistry::set(sessions_manager_addr);

    // Start EpochManager actor
    let epoch_manager_addr = EpochManager::default().start();
    SystemRegistry::set(epoch_manager_addr);

    // Start ChainManager actor
    let chain_manager_addr = ChainManager::default().start();
    SystemRegistry::set(chain_manager_addr);

    // Start InventoryManager actor
    let inventory_manager_addr = InventoryManager::default().start();
    SystemRegistry::set(inventory_manager_addr);

    // Start RadManager actor
    let rad_manager_addr = RadManager::default().start();
    SystemRegistry::set(rad_manager_addr);

    // Start JSON RPC server
    let json_rpc_server_addr = JsonRpcServer::default().start();
    SystemRegistry::set(json_rpc_server_addr);

    // Run system
    system.run()?;

    Ok(())
}

/// Function to close the main system
pub fn close() {
    log::info!("Closing node");

    // FIXME(#72): find out how to gracefully stop the system
    // System::current().stop();

    // Process exit
    exit(0);
}
