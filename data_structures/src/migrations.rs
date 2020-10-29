use serde::{Serialize, Deserialize};
use std::collections::HashMap;

mod v0 {
    use super::*;
    use crate::chain::ChainInfo;
    use crate::data_request::DataRequestPool;
    use crate::chain::Blockchain;
    use crate::chain::AltKeys;
    use crate::chain::NodeStats;
    use crate::chain::ReputationEngine;
    use crate::chain::OutputPointer;
    use crate::chain::ValueTransferOutput;
    use crate::utxo_pool::OwnUnspentOutputsPool;
    use crate::superblock::SuperBlockState;

    /// Unspent Outputs Pool
    #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
    pub struct UnspentOutputsPool {
        /// Map of output pointer to a tuple of:
        /// * Value transfer output
        /// * The number of the block that included the transaction
        ///   (how many blocks were consolidated before this one).
        map: HashMap<OutputPointer, (ValueTransferOutput, u32)>,
    }

    /// Blockchain state (valid at a certain epoch)
    #[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
    pub struct ChainState {
        /// Blockchain information data structure
        pub chain_info: Option<ChainInfo>,
        /// Unspent Outputs Pool
        pub unspent_outputs_pool: UnspentOutputsPool,
        /// Collection of state structures for active data requests
        pub data_request_pool: DataRequestPool,
        /// List of consolidated blocks by epoch
        pub block_chain: Blockchain,
        /// List of unspent outputs that can be spent by this node
        /// Those UTXOs have a timestamp value to avoid double spending
        pub own_utxos: OwnUnspentOutputsPool,
        /// Reputation engine
        pub reputation_engine: Option<ReputationEngine>,
        /// Node mining stats
        pub node_stats: NodeStats,
        /// Alternative public key mapping
        pub alt_keys: AltKeys,
        /// Current superblock state
        pub superblock_state: SuperBlockState,
    }
}
