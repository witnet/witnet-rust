use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Arc, RwLock},
};

pub use actix::System;
use actix::{Actor, SystemRegistry};
use witnet_config::config::Config;
use witnet_validations::witnessing::validate_witnessing_config;

use crate::{
    actors::{
        chain_manager::ChainManager, connections_manager::ConnectionsManager,
        epoch_manager::EpochManager, inventory_manager::InventoryManager, json_rpc::JsonRpcServer,
        peers_manager::PeersManager, rad_manager::RadManager, sessions_manager::SessionsManager,
    },
    config_mngr, signature_mngr, storage_mngr,
    utils::Force,
};

/// Function to run the main system
pub fn run(config: Arc<Config>, ops: NodeOps, callback: fn()) -> Result<(), failure::Error> {
    // Init system
    let system = System::new();

    // Perform some initial validations on the configuration
    let witnessing_config = config.witnessing.clone().into_config();
    let witnessing_config =
        validate_witnessing_config::<String, witnet_rad::Uri>(&witnessing_config)?;

    // JSONRPC server is initialized early because of tokio runtimes shenanigans
    let jsonrpc_runtime = tokio::runtime::Runtime::new().unwrap();
    let jsonrpc_server =
        JsonRpcServer::from_config(&config).initialize(jsonrpc_runtime.handle().clone())?;

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
        let mut cm = ChainManager::default();
        cm.put_node_ops(ops);
        let chain_manager_addr = cm.start();
        SystemRegistry::set(chain_manager_addr);

        // Start InventoryManager actor
        let inventory_manager_addr = InventoryManager::default().start();
        SystemRegistry::set(inventory_manager_addr);

        // Start RadManager actor
        let rad_manager_addr = RadManager::from_config(witnessing_config).start();
        SystemRegistry::set(rad_manager_addr);

        // Start JSON RPC server
        let json_rpc_server_addr = jsonrpc_server.start();
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

/// Special operations that the node can execute.
///
/// Most often, these operations will be executed at an early stage after starting the node, even
/// before some actors go totally live.
#[derive(Clone, Eq, Hash, PartialEq)]
pub enum NodeOp {
    /// Import a chain snapshot from a file.
    SnapshotExport(Force<PathBuf>),
    /// Import a chain snapshot from a file.
    SnapshotImport(Force<PathBuf>),
}

/// A list of `NodeOp` items to perform.
pub struct NodeOps(Arc<RwLock<HashSet<NodeOp>>>);

impl NodeOps {
    /// Add an operation.
    pub fn add(&mut self, op: NodeOp) {
        let mut ops = self.0.write().unwrap();
        (*ops).insert(op);
    }

    /// Tell whether the list of operations contains some operation in particular.
    pub fn contains<F, T>(&self, f: F) -> Option<T>
    where
        F: FnMut(&NodeOp) -> Option<T>,
        T: Clone,
    {
        self.0.read().unwrap().iter().find_map(f)
    }

    /// Tell whether the list of operations contains a chain snapshot export operation.
    pub fn snapshot_export(&self) -> Force<PathBuf> {
        self.contains(|op| match op {
            NodeOp::SnapshotExport(path) => Some(path.clone()),
            _ => None,
        })
        .into()
    }

    /// Tell whether the list of operations contains a chain snapshot import operation.
    pub fn snapshot_import(&self) -> Force<PathBuf> {
        self.contains(|op| match op {
            NodeOp::SnapshotImport(path) => Some(path.clone()),
            _ => None,
        })
        .into()
    }
}

impl Default for NodeOps {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(HashSet::new())))
    }
}

/// Trait defining the behavior of stuff that can take `NodeOps` into account.
///
/// This is meant to be implemented by the node actors.
pub trait PutNodeOps {
    /// Provide an instance of the implementor that has somehow processed the `NodeOps` data.
    fn put_node_ops(&mut self, ops: NodeOps);
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_nodeops_snapshot_export() {
        let mut ops = NodeOps::default();
        assert_eq!(ops.snapshot_export(), Force::None);

        let path = PathBuf::from("./whatever.bin");
        ops.add(NodeOp::SnapshotExport(Force::Some(path.clone())));
        assert_eq!(ops.snapshot_export(), Force::Some(path.clone()));

        ops = NodeOps::default();
        ops.add(NodeOp::SnapshotExport(Force::All(path.clone())));
        assert_eq!(ops.snapshot_export(), Force::All(path));
    }

    #[test]
    fn test_nodeops_snapshot_import() {
        let mut ops = NodeOps::default();
        assert_eq!(ops.snapshot_import(), Force::None);

        let path = PathBuf::from("./whatever.bin");
        ops.add(NodeOp::SnapshotImport(Force::Some(path.clone())));
        assert_eq!(ops.snapshot_import(), Force::Some(path.clone()));

        ops = NodeOps::default();
        ops.add(NodeOp::SnapshotImport(Force::All(path.clone())));
        assert_eq!(ops.snapshot_import(), Force::All(path));
    }
}
