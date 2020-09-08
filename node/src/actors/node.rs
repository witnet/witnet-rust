use std::sync::Arc;

pub use actix::System;
use actix::{Actor, SystemRegistry};

use crate::{
    actors::{
        chain_manager::ChainManager, connections_manager::ConnectionsManager,
        epoch_manager::EpochManager, inventory_manager::InventoryManager, json_rpc::JsonRpcServer,
        peers_manager::PeersManager, rad_manager::RadManager, sessions_manager::SessionsManager,
    },
    config_mngr, signature_mngr, storage_mngr,
};
use witnet_config::config::Config;

/// Function to run the main system
pub fn run(config: Arc<Config>, callback: fn()) -> Result<(), failure::Error> {
    // Init system
    let system = System::new();

    // Init actors
    system.block_on(async {
        // Call cb function (register interrupt handlers)
        callback();

        // Start ConfigManager actor
        config_mngr::start(config.clone());

        // Start StorageManager actor & SignatureManager
        storage_mngr::start();
        signature_mngr::start();

        // Start PeersManager actor
        let peers_manager_addr = PeersManager::from_config(&config).start();
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
        let rad_manager_addr =
        RadManager::with_proxies(config.connections.retrieval_proxies.clone()).start();
        SystemRegistry::set(rad_manager_addr);

        // Start JSON RPC server
        let json_rpc_server_addr = JsonRpcServer::default().start();
        SystemRegistry::set(json_rpc_server_addr);
    });

    // Run system
    system.run().map_err(|error| error.into())
}

/// Function to close the main system
pub fn close(system: &System) {
    log::info!("Closing node");

    system.stop();
}
