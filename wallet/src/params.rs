use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use witnet_crypto::hash::HashFunction;
use witnet_data_structures::{
    chain::{CheckpointBeacon, ConsensusConstants, EpochConstants, Hash},
    witnessing::WitnessingConfig,
};
use witnet_net::client::tcp::jsonrpc::Subscribe;

use crate::{actors::app::NodeClient, types};

/// Initialization parameters that can be specific for each wallet.
#[derive(Clone)]
pub struct Params {
    pub testnet: bool,
    pub seed_password: types::Password,
    pub master_key_salt: Vec<u8>,
    pub id_hash_iterations: u32,
    pub id_hash_function: HashFunction,
    pub db_hash_iterations: u32,
    pub db_iv_length: usize,
    pub db_salt_length: usize,
    pub epoch_constants: EpochConstants,
    pub node_sync_batch_size: u32,
    pub genesis_hash: Hash,
    pub genesis_prev_hash: Hash,
    pub sync_address_batch_length: u16,
    pub max_vt_weight: u32,
    pub max_dr_weight: u32,
    pub consensus_constants: ConsensusConstants,
    pub use_unconfirmed_utxos: bool,
    pub pending_transactions_timeout_seconds: u64,
    pub witnessing: WitnessingConfig<witnet_net::Uri>,
}

#[derive(Clone)]
pub struct NodeParams {
    /// Reference to the JSON-RPC client actor.
    pub client: Arc<NodeClient>,
    /// A reference to the latest block that the node has consolidated into its block chain.
    pub last_beacon: Arc<RwLock<CheckpointBeacon>>,
    /// The name of the network in which the node is operating.
    pub network: String,
    /// Timeout for JSON-RPC requests sent to the node.
    pub requests_timeout: Duration,
    /// Subscriptions to real time notifications from the node.
    pub subscriptions: Arc<Mutex<HashMap<String, Subscribe>>>,
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

    /// Get an existing JsonRpcClient actor.
    ///
    /// This method exists for convenience in case that at some point we decide to allow changing
    /// the `JsonRpcClient` address by putting `NodeClient` inside an `Arc<RwLock<_>>` or similar.
    #[inline(always)]
    pub fn get_client(&self) -> Arc<NodeClient> {
        self.client.clone()
    }
}
