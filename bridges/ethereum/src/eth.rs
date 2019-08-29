use crate::config::Config;
use bimap::BiMap;
use ethabi::{Bytes, Token};
use futures::Future;
use futures_locks::RwLock;
use log::*;
use std::collections::{HashMap, HashSet};
use web3::{
    contract::Contract,
    types::{H160, H256, U256},
};
use witnet_data_structures::chain::Hash;

/// State of a data request in the WBI contract, including local intermediate states
#[derive(Debug)]
pub enum DrState {
    /// The data request was just posted, and may be available for claiming
    Posted,
    /// The node sent a transaction to claim this data request, but that transaction
    /// is not yet confirmed. This state prevents the node from double-claiming the
    /// same data request multiple times in parallel.
    Claiming,
    /// The data request was claimed by this node.
    /// Data requests claimed by other nodes are in `Posted` state, so we can
    /// try to claim it in the future.
    Claimed,
    /// The data request was included in a Witnet block, and the proof of inclusion
    /// was sent to Ethereum, but it was not yet included in an Ethereum block.
    Including {
        /// Proof of inclusion: lemma
        poi: Vec<U256>,
        /// Proof of inclusion: index
        poi_index: U256,
        /// Hash of the block containing the data request
        block_hash: U256,
    },
    /// The data request was included in a Witnet block, and the reward for doing so
    /// was already paid to the bridge node. The data request is now waiting to be
    /// resolved.
    Included,
    /// The data request was resolved in a Witnet block, and the proof of inclusion
    /// was sent to Ethereum, but it was not yet included in an Ethereum block.
    Resolving {
        /// Proof of inclusion: lemma
        poi: Vec<U256>,
        /// Proof of inclusion: index
        poi_index: U256,
        /// Hash of the block containing the data request
        block_hash: U256,
        /// Result of the data request, serialized as CBOR
        result: Bytes,
    },
    /// The data request was resolved in a Witnet block, and the reward for doing so
    /// (the tally fee) was already paid to the node that reported the result.
    Resolved {
        /// Result of the data request, serialized as CBOR
        result: Vec<u8>,
    },
}

/// List of all the data requests posted to the WBI, categorized by state.
/// This allows for an efficient functionality of the bridge.
#[derive(Debug, Default)]
pub struct WbiRequests {
    requests: HashMap<U256, DrState>,
    posted: HashSet<U256>,
    claiming: HashSet<U256>,
    // Claimed by our node, used to reportInclusion
    // dr_output_hash: Hash
    claimed: BiMap<U256, Hash>,
    including: HashSet<U256>,
    // dr_tx_hash: Hash
    included: BiMap<U256, Hash>,
    resolving: HashSet<U256>,
    resolved: HashSet<U256>,
}

impl WbiRequests {
    fn remove_from_all_helper_maps(&mut self, dr_id: U256) {
        self.posted.remove(&dr_id);
        self.claiming.remove(&dr_id);
        self.claimed.remove_by_left(&dr_id);
        self.including.remove(&dr_id);
        self.included.remove_by_left(&dr_id);
        self.resolving.remove(&dr_id);
        self.resolved.remove(&dr_id);
    }
    /// Insert a data request in `Posted` state
    pub fn insert_posted(&mut self, dr_id: U256) {
        // This is only safe if the data request did not exist yet
        match self.requests.get(&dr_id) {
            None => {
                self.remove_from_all_helper_maps(dr_id);
                self.requests.insert(dr_id, DrState::Posted);
                self.posted.insert(dr_id);
            }
            Some(DrState::Posted) => {
                debug!("Invalid state in WbiRequests: [{}] was being set to Posted, but it is already Posted", dr_id);
            }
            _ => {
                warn!(
                    "Invalid state in WbiRequests: [{}] was being set to Posted, but it is: {:?}",
                    dr_id, self.requests[&dr_id]
                );
            }
        }
    }
    /// Insert a data request in `Included` state, with the data request
    /// transaction hash from Witnet stored to allow a map
    /// from WBI_dr_id to Witnet_dr_tx_hash
    pub fn insert_included(&mut self, dr_id: U256, dr_tx_hash: Hash) {
        // This is only safe if the data request was
        // in a state "before" Included
        match self.requests.get(&dr_id) {
            None
            | Some(DrState::Posted)
            | Some(DrState::Claiming)
            | Some(DrState::Claimed)
            | Some(DrState::Including { .. }) => {
                self.remove_from_all_helper_maps(dr_id);
                self.requests.insert(dr_id, DrState::Included);
                self.included.insert(dr_id, dr_tx_hash);
            }
            Some(DrState::Included) => {
                debug!("Invalid state in WbiRequests: [{}] was being set to Included, but it is already Included", dr_id);
            }
            _ => {
                warn!(
                    "Invalid state in WbiRequests: [{}] was being set to Included, but it is: {:?}",
                    dr_id, self.requests[&dr_id]
                );
            }
        }
    }
    /// Insert a data request in `Resolved` state, with the result as a vector
    /// of bytes.
    pub fn insert_result(&mut self, dr_id: U256, result: Vec<u8>) {
        // This is always safe, we can just overwrite the old value if it exists
        self.remove_from_all_helper_maps(dr_id);
        self.requests.insert(dr_id, DrState::Resolved { result });
        self.resolved.insert(dr_id);
    }
    /// Mark this data request as `Including`
    pub fn set_including(
        &mut self,
        dr_id: U256,
        poi: Vec<U256>,
        poi_index: U256,
        block_hash: U256,
    ) {
        self.remove_from_all_helper_maps(dr_id);
        self.requests.insert(
            dr_id,
            DrState::Including {
                poi,
                poi_index,
                block_hash,
            },
        );
        self.including.insert(dr_id);
    }
    /// Mark this data request as `Claiming`
    pub fn set_claiming(&mut self, dr_id: U256) {
        self.remove_from_all_helper_maps(dr_id);
        self.requests.insert(dr_id, DrState::Claiming);
        self.claiming.insert(dr_id);
    }
    /// Mark this data request as `Resolving`
    pub fn set_resolving(
        &mut self,
        dr_id: U256,
        poi: Vec<U256>,
        poi_index: U256,
        block_hash: U256,
        result: Bytes,
    ) {
        self.remove_from_all_helper_maps(dr_id);
        self.requests.insert(
            dr_id,
            DrState::Resolving {
                poi,
                poi_index,
                block_hash,
                result,
            },
        );
        self.resolving.insert(dr_id);
    }
    /// If the data request is in claiming state, undo the claim.
    /// Otherwise, do nothing.
    pub fn undo_claim(&mut self, dr_id: U256) {
        if self.claiming.remove(&dr_id) {
            self.requests.insert(dr_id, DrState::Posted);
            self.posted.insert(dr_id);
        }
    }
    /// If the data request is in claiming state, confirm the claim.
    /// Otherwise, do nothing.
    pub fn confirm_claim(&mut self, dr_id: U256, dr_output_hash: Hash) {
        // If the data request is in claiming state, confirm the claim
        // Otherwise, do nothing
        if self.claiming.remove(&dr_id) {
            self.requests.insert(dr_id, DrState::Claimed);
            self.claimed.insert(dr_id, dr_output_hash);
        }
    }
    /// View of all the data requests in `Posted` state.
    pub fn posted(&self) -> &HashSet<U256> {
        &self.posted
    }
    /// View of all the data requests in `Claimed` state, with an auxiliar
    /// `dr_output_hash`.
    pub fn claimed(&self) -> &BiMap<U256, Hash> {
        &self.claimed
    }
    /// View of all the data requests in `Claimed` state, with an auxiliar
    /// `dr_tx_hash`
    pub fn included(&self) -> &BiMap<U256, Hash> {
        &self.included
    }
    /// View of all the data requests indexed by id
    pub fn requests(&self) -> &HashMap<U256, DrState> {
        &self.requests
    }
}

/// State needed to interact with the ethereum side of the bridge
#[derive(Debug)]
pub struct EthState {
    /// Web3 event loop handle
    pub eloop: web3::transports::EventLoopHandle,
    /// Web3
    pub web3: web3::Web3<web3::transports::Http>,
    /// Accounts
    pub accounts: Vec<H160>,
    /// WBI contract
    pub wbi_contract: Contract<web3::transports::Http>,
    /// PostDataRequest event signature
    pub post_dr_event_sig: H256,
    /// InclusionDataRequest event signature
    pub inclusion_dr_event_sig: H256,
    /// PostResult event signature
    pub post_tally_event_sig: H256,
    /// BlockRelay contract
    pub block_relay_contract: Contract<web3::transports::Http>,
    /// Internal state of the WBI
    pub wbi_requests: RwLock<WbiRequests>,
}

impl EthState {
    /// Read addresses from config and create `State` struct
    pub fn create(config: &Config) -> Result<Self, String> {
        info!(
            "Connecting to Ethereum node running at {}",
            config.eth_client_url
        );
        let (eloop, web3_http) = web3::transports::Http::new(&config.eth_client_url)
            .map_err(|e| format!("Failed to connect to Ethereum client.\nError: {:?}", e))?;
        let web3 = web3::Web3::new(web3_http);
        let accounts = web3
            .eth()
            .accounts()
            .wait()
            .map_err(|e| format!("Unable to get list of available accounts: {:?}", e))?;
        debug!("Web3 accounts: {:?}", accounts);

        // Why read files at runtime when you can read files at compile time
        let wbi_contract_abi_json: &[u8] = include_bytes!("../wbi_abi.json");
        let wbi_contract_abi = ethabi::Contract::load(wbi_contract_abi_json)
            .map_err(|e| format!("Unable to load WBI contract from ABI: {:?}", e))?;
        let wbi_contract_address = config.wbi_contract_addr;
        let wbi_contract =
            Contract::new(web3.eth(), wbi_contract_address, wbi_contract_abi.clone());

        let block_relay_contract_abi_json: &[u8] = include_bytes!("../block_relay_abi.json");
        let block_relay_contract_abi = ethabi::Contract::load(block_relay_contract_abi_json)
            .map_err(|e| format!("Unable to load BlockRelay contract from ABI: {:?}", e))?;
        let block_relay_contract_address = config.block_relay_contract_addr;
        let block_relay_contract = Contract::new(
            web3.eth(),
            block_relay_contract_address,
            block_relay_contract_abi.clone(),
        );

        debug!("WBI events: {:?}", wbi_contract_abi.events);
        let post_dr_event = wbi_contract_abi
            .event("PostedRequest")
            .map_err(|e| format!("Unable to get PostedRequest event: {:?}", e))?
            .clone();
        let inclusion_dr_event = wbi_contract_abi
            .event("IncludedRequest")
            .map_err(|e| format!("Unable to get IncludedRequest event: {:?}", e))?
            .clone();
        let post_tally_event = wbi_contract_abi
            .event("PostedResult")
            .map_err(|e| format!("Unable to get PostedResult event: {:?}", e))?
            .clone();

        let post_dr_event_sig = post_dr_event.signature();
        let inclusion_dr_event_sig = inclusion_dr_event.signature();
        let post_tally_event_sig = post_tally_event.signature();

        let wbi_requests = RwLock::new(Default::default());

        Ok(Self {
            eloop,
            web3,
            accounts,
            wbi_contract,
            block_relay_contract,
            post_dr_event_sig,
            inclusion_dr_event_sig,
            post_tally_event_sig,
            wbi_requests,
        })
    }
}

/// Assume the first return value of an event log is a U256 and return it
pub fn read_u256_from_event_log(value: &web3::types::Log) -> Result<U256, ()> {
    let event_types = vec![ethabi::ParamType::Uint(0)];
    let event_data = ethabi::decode(&event_types, &value.data.0);
    debug!("Event data: {:?}", event_data);

    // Errors are handled by the caller, if this fails there is nothing we can do
    match event_data.map_err(|_| ())?.get(0).ok_or(())? {
        Token::Uint(x) => Ok(*x),
        _ => Err(()),
    }
}

/// Possible ethereum events emited by the WBI ethereum contract
pub enum WbiEvent {
    /// A new data request has been posted to ethereum
    PostedRequest(U256),
    /// A data request from ethereum has been posted to witnet with a proof of
    /// inclusion in a block
    IncludedRequest(U256),
    /// A data request has been resolved in witnet, and the result was reported
    /// to ethereum with a proof of inclusion
    PostedResult(U256),
}
