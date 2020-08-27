use std::sync::Arc;
use std::{sync::RwLock, time::Duration};

use actix::Addr;

use witnet_data_structures::chain::{CheckpointBeacon, EpochConstants, Hash};
use witnet_net::client::tcp::{Error as TcpError, JsonRpcClient};

use crate::types;

/// Initialization parameters that can be specific for each wallet.
#[derive(Clone)]
pub struct Params {
    pub testnet: bool,
    pub seed_password: types::Password,
    pub master_key_salt: Vec<u8>,
    pub id_hash_iterations: u32,
    pub id_hash_function: types::HashFunction,
    pub db_hash_iterations: u32,
    pub db_iv_length: usize,
    pub db_salt_length: usize,
    pub epoch_constants: EpochConstants,
    pub node_sync_batch_size: u32,
    pub genesis_prev_hash: Hash,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            testnet: false,
            seed_password: "".into(),
            master_key_salt: b"Bitcoin seed".to_vec(),
            id_hash_iterations: 4096,
            id_hash_function: types::HashFunction::Sha256,
            db_hash_iterations: 10_000,
            db_iv_length: 16,
            db_salt_length: 32,
            epoch_constants: EpochConstants::default(),
            node_sync_batch_size: 100,
            genesis_prev_hash: Hash::default(),
        }
    }
}

#[derive(Clone)]
pub struct NodeParams {
    /// The IP address and port of the node we are connecting to.
    pub address: String,
    /// Reference to the JSON-RPC client actor.
    pub client: Addr<JsonRpcClient>,
    /// A reference to the latest block that the node has consolidated into its block chain.
    pub last_beacon: Arc<RwLock<CheckpointBeacon>>,
    /// The name of the network in which the node is operating.
    pub network: String,
    /// Timeout for JSON-RPC requests sent to the node.
    pub requests_timeout: Duration,
}

impl NodeParams {
    /// Retrieve the `last_beacon` field.
    /// This panics if the `RwLock` is poisoned.
    pub fn get_last_beacon(&self) -> CheckpointBeacon {
        let lock = (*self.last_beacon).read();
        *lock.expect("Read locks should only fail if poisoned.")
    }

    /// Update the `last_beacon` field with the information of the latest block that the node has
    /// consolidated into its block chain.
    /// This is a best-effort method. It will silently do nothing if the write lock on `last_beacon`
    /// cannot be acquired or if the new beacon looks older than the current one.  
    pub fn update_last_beacon(&self, new_beacon: CheckpointBeacon) {
        let lock = (*self.last_beacon).write();
        if let Ok(mut beacon) = lock {
            if new_beacon.checkpoint > beacon.checkpoint {
                *beacon = new_beacon
            }
        }
    }

    /// Get the address of an existing JsonRpcClient actor, or that of a fresh new actor if the
    /// existing one had died.  
    pub fn get_client(&mut self) -> Result<Addr<JsonRpcClient>, TcpError> {
        if !self.client.connected() {
            log::warn!("JsonRpcClient actor was dead. Replacing...");
            self.client = JsonRpcClient::start(&self.address)?;
        }

        Ok(self.client.clone())
    }
}
