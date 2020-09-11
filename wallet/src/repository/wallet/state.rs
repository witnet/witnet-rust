use super::*;
use witnet_data_structures::chain::EpochConstants;

/// A single wallet state. It includes:
///  - fields required to operate wallet accounts (e.g. derive addresses)
///  - on-memory state after indexing pending block transactions
pub struct State {
    /// Current account index
    pub account: u32,
    /// Available account indices
    pub available_accounts: Vec<u32>,
    /// Current wallet balance (including pending movements)
    pub balance: u64,
    /// Wallet caption
    pub caption: Option<String>,
    /// Epoch constants
    pub epoch_constants: EpochConstants,
    /// Keychains used to derive addresses
    pub keychains: [types::ExtendedSK; 2],
    /// Beacon of last block confirmed by superblock (or during sync process)
    pub last_confirmed: CheckpointBeacon,
    /// Beacon of the last block received during synchronization
    pub last_sync: CheckpointBeacon,
    /// Wallet name
    pub name: Option<String>,
    /// Next external index used to derive addresses
    pub next_external_index: u32,
    /// Next internal index used to derive addresses
    pub next_internal_index: u32,
    /// List of pending address infos, waiting to be confirmed with a superblock
    pub pending_address_infos: HashMap<String, Vec<model::AddressInfo>>,
    /// List of pending blocks waiting to be confirmed
    pub pending_blocks: HashMap<String, model::Beacon>,
    /// List of pending balance movements, waiting to be confirmed with a superblock
    pub pending_movements: HashMap<String, Vec<model::BalanceMovement>>,
    /// Next transaction identifier of the wallet
    pub transaction_next_id: u32,
    /// Current UTXO set (including pending movements)
    pub utxo_set: model::UtxoSet,
}
