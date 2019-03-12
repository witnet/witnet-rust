//! # ChainManager actor
//!
//! This module contains the ChainManager actor which is in charge
//! of managing the blocks and transactions of the Witnet blockchain
//! received through the protocol, and also encapsulates the logic of the
//! _unspent transaction outputs_.
//!
//! Among its responsibilities are the following:
//!
//! * Initializing the chain info upon running the node for the first time and persisting it into storage [StorageManager](actors::storage_manager::StorageManager)
//! * Recovering the chain info from storage and keeping it in its state.
//! * Validating block candidates as they come from a session.
//! * Consolidating multiple block candidates for the same checkpoint into a single valid block.
//! * Putting valid blocks into storage by sending them to the inventory manager actor.
//! * Having a method for letting other components get blocks by *hash* or *checkpoint*.
//! * Having a method for letting other components get the epoch of the current tip of the
//! blockchain (e.g. the last epoch field required for the handshake in the Witnet network
//! protocol).
//! * Validating transactions as they come from any [Session](actors::session::Session). This includes:
//!     - Iterating over its inputs, adding the value of the inputs to calculate the value of the transaction.
//!     - Running the output scripts, expecting them all to return `TRUE` and leave an empty stack.
//!     - Verifying that the sum of all inputs is greater than or equal to the sum of all the outputs.
//! * Keeping valid transactions into memory. This in-memory transaction pool is what we call the _mempool_. Valid transactions are immediately appended to the mempool.
//! * Keeping every unspent transaction output (UTXO) in the block chain in memory. This is called the _UTXO set_.
//! * Updating the UTXO set with valid transactions that have already been anchored into a valid block. This includes:
//!     - Removing the UTXOs that the transaction spends as inputs.
//!     - Adding a new UTXO for every output in the transaction.
use std::collections::HashMap;

use actix::{
    ActorFuture, AsyncContext, Context, ContextFutureSpawner, Supervised, System, SystemService,
    WrapFuture,
};
use ansi_term::Color::{Purple, White, Yellow};
use log::{debug, error, info, warn};
use witnet_rad::types::RadonTypes;

use witnet_data_structures::{
    chain::{
        ActiveDataRequestPool, Block, ChainState, CheckpointBeacon, DataRequestReport, Epoch, Hash,
        Hashable, InventoryItem, Output, OutputPointer, Transaction, TransactionsPool,
        UnspentOutputsPool,
    },
    serializers::decoders::TryFrom,
};
use witnet_storage::{error::StorageError, storage::Storable};
use witnet_util::error::WitnetError;

use self::{
    data_request::DataRequestPool,
    validations::{validate_block, validate_candidate},
};
use crate::actors::{
    inventory_manager::InventoryManager,
    messages::{AddItem, AddTransaction, Broadcast, Put, SendInventoryItem},
    sessions_manager::SessionsManager,
    storage_keys::CHAIN_STATE_KEY,
    storage_manager::StorageManager,
};

mod actor;
mod data_request;
mod handlers;
mod mining;
mod validations;

/// Maximum blocks number to be sent during synchronization process
pub const MAX_BLOCKS_SYNC: usize = 500;

/// Possible errors when interacting with ChainManager
#[derive(Debug)]
pub enum ChainManagerError {
    /// A block being processed was already known to this node
    BlockAlreadyExists,
    /// A block does not exist
    BlockDoesNotExist,
    /// StorageError
    StorageError(WitnetError<StorageError>),
}

impl From<WitnetError<StorageError>> for ChainManagerError {
    fn from(x: WitnetError<StorageError>) -> Self {
        ChainManagerError::StorageError(x)
    }
}

/// State Machine
#[derive(Debug)]
pub enum StateMachine {
    /// First state, ChainManager is waiting to consensus between its peers
    WaitingConsensus,
    /// Second state, ChainManager synchronization process
    Synchronizing,
    /// Third state, ChainManager is ready to mine and consolidated blocks
    Synced,
}

impl Default for StateMachine {
    fn default() -> Self {
        StateMachine::WaitingConsensus
    }
}

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR BASIC STRUCTURE
////////////////////////////////////////////////////////////////////////////////////////
/// ChainManager actor
#[derive(Default)]
pub struct ChainManager {
    /// Blockchain state data structure
    chain_state: ChainState,
    /// Current Epoch
    current_epoch: Option<Epoch>,
    /// Transactions Pool (_mempool_)
    transactions_pool: TransactionsPool,
    /// Maximum weight each block can have
    max_block_weight: u32,
    // Random value to help with debugging because there is no signature
    // and all the mined blocks have the same hash.
    // This random value helps to distinguish blocks mined on different nodes
    // To be removed when we implement real signing.
    // TODO: Remove after create signatures
    random: u64,
    /// Mining enabled
    mining_enabled: bool,
    /// Hash of the genesis block
    genesis_block_hash: Hash,
    /// Pool of active data requests
    data_request_pool: DataRequestPool,
    /// state of the state machine
    sm_state: StateMachine,
    /// The best beacon known to this node—to which it will try to catch up
    target_beacon: Option<CheckpointBeacon>,
    /// Map that stores candidate blocks for further validation and consolidation as tip of the blockchain
    candidates: HashMap<Hash, Block>,
}

/// Struct that keeps a block candidate and its modifications in the blockchain
#[derive(Debug, Clone)]
pub struct BlockInChain {
    block: Block,
    utxo_set: UnspentOutputsPool,
    data_request_pool: DataRequestPool,
}

/// Required trait for being able to retrieve ChainManager address from registry
impl Supervised for ChainManager {}

/// Required trait for being able to retrieve ChainManager address from registry
impl SystemService for ChainManager {}

/// Auxiliary methods for ChainManager actor
impl ChainManager {
    /// Method to persist chain_info into storage
    fn persist_chain_state(&self, ctx: &mut Context<Self>) {
        // Get StorageManager address
        let storage_manager_addr = System::current().registry().get::<StorageManager>();

        match self.chain_state.chain_info.as_ref() {
            Some(x) => x,
            None => {
                error!("Trying to persist an empty chain state value");
                return;
            }
        };

        // Persist chain_info into storage. `AsyncContext::wait` registers
        // future within context, but context waits until this future resolves
        // before processing any other events.
        let msg = Put::from_value(CHAIN_STATE_KEY, &self.chain_state).unwrap();
        storage_manager_addr
            .send(msg)
            .into_actor(self)
            .then(|res, _act, _ctx| {
                match res {
                    Ok(Ok(_)) => debug!("Successfully persisted chain_info into storage"),
                    _ => {
                        error!("Failed to persist chain_info into storage");
                        // FIXME(#72): handle errors
                    }
                }
                actix::fut::ok(())
            })
            .wait(ctx);
    }

    /// Method to Send an Item to Inventory Manager
    fn persist_item(&self, ctx: &mut Context<Self>, item: InventoryItem) {
        // Get InventoryManager address
        let inventory_manager_addr = System::current().registry().get::<InventoryManager>();

        // Persist block into storage through InventoryManager. `AsyncContext::wait` registers
        // future within context, but context waits until this future resolves
        // before processing any other events.
        inventory_manager_addr
            .send(AddItem { item })
            .into_actor(self)
            .then(|res, _act, _ctx| match res {
                // Process the response from InventoryManager
                Err(e) => {
                    // Error when sending message
                    error!("Unsuccessful communication with InventoryManager: {}", e);
                    actix::fut::err(())
                }
                Ok(res) => match res {
                    Err(e) => {
                        // InventoryManager error
                        error!("Error while getting block from InventoryManager: {}", e);
                        actix::fut::err(())
                    }
                    Ok(_) => actix::fut::ok(()),
                },
            })
            .wait(ctx)
    }

    /// Method to persist a Data Request into the Storage
    fn persist_data_request(
        &self,
        ctx: &mut Context<Self>,
        (output_pointer, data_request_report): &(OutputPointer, DataRequestReport),
    ) {
        // Get StorageManager address
        let storage_manager_addr = System::current().registry().get::<StorageManager>();

        // Persist block_chain into storage. `AsyncContext::wait` registers
        // future within context, but context waits until this future resolves
        // before processing any other events.
        let msg = Put::from_value(output_pointer.to_bytes().unwrap(), data_request_report).unwrap();
        storage_manager_addr
            .send(msg)
            .into_actor(self)
            .then(|res, _act, _ctx| {
                match res {
                    Ok(Ok(_)) => debug!("Successfully persisted block_chain into storage"),
                    _ => {
                        error!("Failed to persist block_chain into storage");
                        // FIXME(#72): handle errors
                    }
                }
                actix::fut::ok(())
            })
            .wait(ctx);
    }

    fn broadcast_item(&self, item: InventoryItem) {
        // Get SessionsManager address
        let sessions_manager_addr = System::current().registry().get::<SessionsManager>();

        sessions_manager_addr.do_send(Broadcast {
            command: SendInventoryItem { item },
            only_inbound: false,
        });
    }

    fn process_requested_block(&mut self, ctx: &mut Context<Self>, block: Block) -> Result<(), ()> {
        if let (Some(current_epoch), Some(chain_info)) =
            (self.current_epoch, self.chain_state.chain_info.as_ref())
        {
            let chain_beacon = chain_info.highest_block_checkpoint;

            if let Ok(block_in_chain) = validate_block(
                &block,
                current_epoch,
                chain_beacon,
                self.genesis_block_hash,
                &self.chain_state.unspent_outputs_pool,
                &self.transactions_pool,
                &self.data_request_pool,
            ) {
                // Persist block and update ChainState
                self.consolidate_block(
                    ctx,
                    block_in_chain.block,
                    block_in_chain.utxo_set,
                    block_in_chain.data_request_pool,
                    false,
                );

                Ok(())
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    }

    fn process_candidate(&mut self, block: Block) {
        if let Some(current_epoch) = self.current_epoch {
            let hash_block = block.hash();

            if !self.candidates.contains_key(&hash_block)
                && validate_candidate(&block, current_epoch).is_ok()
            {
                self.candidates.insert(hash_block, block.clone());
                self.broadcast_item(InventoryItem::Block(block));
            }
        } else {
            warn!("ChainManager doesn't have current epoch");
        }
    }

    fn update_transaction_pool(&mut self, transactions: &[Transaction]) {
        for transaction in transactions {
            self.transactions_pool.remove(&transaction.hash());
        }
    }

    fn consolidate_block(
        &mut self,
        ctx: &mut Context<Self>,
        block: Block,
        utxo_set: UnspentOutputsPool,
        dr_pool: DataRequestPool,
        info_flag: bool,
    ) {
        // Update chain_info
        match self.chain_state.chain_info.as_mut() {
            Some(chain_info) => {
                let block_hash = block.hash();
                let block_epoch = block.block_header.beacon.checkpoint;

                // Update `highest_block_checkpoint`
                let beacon = CheckpointBeacon {
                    checkpoint: block_epoch,
                    hash_prev_block: block_hash,
                };
                chain_info.highest_block_checkpoint = beacon;

                // Update UnspentOutputsPool
                self.chain_state.unspent_outputs_pool = utxo_set;

                // Update TransactionPool
                self.update_transaction_pool(block.txns.as_ref());

                // Update DataRequestPool
                self.data_request_pool = dr_pool;
                let reveals = self.data_request_pool.update_data_request_stages();
                for reveal in reveals {
                    // Send AddTransaction message to self
                    // And broadcast it to all of peers
                    ctx.address().do_send(AddTransaction {
                        transaction: reveal,
                    })
                }
                // Persist finished data requests into storage
                let to_be_stored = self.data_request_pool.finished_data_requests();
                to_be_stored.into_iter().for_each(|dr| {
                    self.persist_data_request(ctx, &dr);
                    if info_flag {
                        self.show_info_tally(dr, block_epoch);
                    }
                });
                // FIXME: Revisit to avoid data redundancies
                // Store active data requests
                self.chain_state.data_request_pool = ActiveDataRequestPool {
                    waiting_for_reveal: self.data_request_pool.waiting_for_reveal.clone(),
                    data_requests_by_epoch: self.data_request_pool.data_requests_by_epoch.clone(),
                    data_request_pool: self.data_request_pool.data_request_pool.clone(),
                    to_be_stored: self.data_request_pool.to_be_stored.clone(),
                    dr_pointer_cache: self.data_request_pool.dr_pointer_cache.clone(),
                };
                if info_flag {
                    self.show_info_dr(&block);

                    debug!("{:?}", block);
                    debug!("Mint transaction hash: {:?}", block.txns[0].hash());
                }

                // Insert candidate block into `block_chain` and persist it
                self.chain_state.block_chain.insert(block_epoch, block_hash);
                self.persist_item(ctx, InventoryItem::Block(block));

                // Persist chain_info into storage
                self.persist_chain_state(ctx);
            }
            None => {
                error!("No ChainInfo loaded in ChainManager");
            }
        }
    }

    fn show_info_tally(&self, dr: (OutputPointer, DataRequestReport), block_epoch: Epoch) {
        let tally_output_pointer = dr.1.tally;
        let tr = self
            .chain_state
            .unspent_outputs_pool
            .get(&tally_output_pointer);
        if let Some(Output::Tally(tally_output)) = tr {
            let result = RadonTypes::try_from(tally_output.result.as_slice())
                .map(|x| x.to_string())
                .unwrap_or_else(|_| "RADError".to_string());
            info!(
                "{} {} completed at epoch #{} with result: {}",
                Yellow.bold().paint("[Data Request]"),
                Yellow.bold().paint(&dr.0.to_string()),
                Yellow.bold().paint(block_epoch.to_string()),
                Yellow.bold().paint(result),
            );
        }
    }

    fn show_info_dr(&self, block: &Block) {
        let block_hash = block.hash();
        let block_epoch = block.block_header.beacon.checkpoint;

        let info =
            self.data_request_pool
                .data_request_pool
                .iter()
                .fold(String::new(), |acc, (k, v)| {
                    format!(
                        "{}\n\t* {} Stage: {}, Commits: {}, Reveals: {}",
                        acc,
                        White.bold().paint(k.to_string()),
                        White.bold().paint(format!("{:?}", v.stage)),
                        v.info.commits.len(),
                        v.info.reveals.len()
                    )
                });

        if info.is_empty() {
            info!(
                "{} Block {} consolidated for epoch #{} {}",
                Purple.bold().paint("[Chain]"),
                Purple.bold().paint(block_hash.to_string()),
                Purple.bold().paint(block_epoch.to_string()),
                White.paint("with no data requests".to_string()),
            );
        } else {
            info!(
                "{} Block {} consolidated for epoch #{}\n{}{}",
                Purple.bold().paint("[Chain]"),
                Purple.bold().paint(block_hash.to_string()),
                Purple.bold().paint(block_epoch.to_string()),
                White.bold().paint("Data Requests: "),
                White.bold().paint(info),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[cfg(test)]
    fn build_hardcoded_block(checkpoint: u32, influence: u64, hash_prev_block: Hash) -> Block {
        use witnet_data_structures::chain::*;
        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: [0; 32],
            s: [0; 32],
            v: 0,
        });
        let keyed_signature = vec![KeyedSignature {
            public_key: [0; 32],
            signature: signature.clone(),
        }];

        let reveal_input = Input::Reveal(RevealInput {
            output_index: 0,
            transaction_id: Hash::SHA256([0; 32]),
        });

        let commit_input = Input::Commit(CommitInput {
            nonce: 0,
            output_index: 0,
            reveal: [0; 32].to_vec(),
            transaction_id: Hash::SHA256([0; 32]),
        });

        let data_request_input = Input::DataRequest(DataRequestInput {
            output_index: 0,
            poe: [0; 32],
            transaction_id: Hash::SHA256([0; 32]),
        });
        let value_transfer_output = Output::ValueTransfer(ValueTransferOutput {
            pkh: [0; 20],
            value: 0,
        });
        let rad_aggregate = RADAggregate { script: vec![0] };
        let rad_retrieve_1 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };
        let rad_retrieve_2 = RADRetrieve {
            kind: RADType::HttpGet,
            url: "https://openweathermap.org/data/2.5/weather?id=2950159&appid=b6907d289e10d714a6e88b30761fae22".to_string(),
            script: vec![0],
        };
        let rad_consensus = RADConsensus { script: vec![0] };
        let rad_deliver_1 = RADDeliver {
            kind: RADType::HttpGet,
            url: "https://hooks.zapier.com/hooks/catch/3860543/l2awcd/".to_string(),
        };
        let rad_deliver_2 = RADDeliver {
            kind: RADType::HttpGet,
            url: "https://hooks.zapier.com/hooks/catch/3860543/l1awcw/".to_string(),
        };
        let rad_request = RADRequest {
            aggregate: rad_aggregate,
            not_before: 0,
            retrieve: vec![rad_retrieve_1, rad_retrieve_2],
            consensus: rad_consensus,
            deliver: vec![rad_deliver_1, rad_deliver_2],
        };
        let data_request_output = Output::DataRequest(DataRequestOutput {
            backup_witnesses: 0,
            commit_fee: 0,
            data_request: rad_request,
            pkh: [0; 20],
            reveal_fee: 0,
            tally_fee: 0,
            time_lock: 0,
            value: 0,
            witnesses: 0,
        });
        let commit_output = Output::Commit(CommitOutput {
            commitment: Hash::SHA256([0; 32]),
            value: 0,
        });
        let reveal_output = Output::Reveal(RevealOutput {
            pkh: [0; 20],
            reveal: [0; 32].to_vec(),
            value: 0,
        });
        let consensus_output = Output::Tally(TallyOutput {
            pkh: [0; 20],
            result: [0; 32].to_vec(),
            value: 0,
        });

        let inputs = vec![commit_input, data_request_input, reveal_input];
        let outputs = vec![
            value_transfer_output,
            data_request_output,
            commit_output,
            reveal_output,
            consensus_output,
        ];
        let txns: Vec<Transaction> = vec![Transaction {
            inputs,
            signatures: keyed_signature,
            outputs,
            version: 0,
        }];
        let proof = LeadershipProof {
            block_sig: Some(signature),
            influence,
        };

        Block {
            block_header: BlockHeader {
                version: 1,
                beacon: CheckpointBeacon {
                    checkpoint,
                    hash_prev_block,
                },
                hash_merkle_root: Hash::SHA256([222; 32]),
            },
            proof,
            txns,
        }
    }

    #[test]
    fn block_storable() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let b = InventoryItem::Block(build_hardcoded_block(0, 0, Hash::SHA256([111; 32])));
        let msp = b.to_bytes().unwrap();
        assert_eq!(InventoryItem::from_bytes(&msp).unwrap(), b);

        println!("{:?}", b);
        println!("{:?}", msp);
        /*
        use witnet_data_structures::chain::Hash::SHA256;
        use witnet_data_structures::chain::Signature::Secp256k1;
        let mined_block = InventoryItem::Block(Block {
            block_header: BlockHeader {
                version: 0,
                beacon: CheckpointBeacon {
                    checkpoint: 400,
                    hash_prev_block: SHA256([
                        47, 17, 139, 130, 7, 164, 151, 185, 64, 43, 88, 183, 53, 213, 38, 89, 76,
                        66, 231, 53, 78, 216, 230, 217, 245, 184, 150, 33, 182, 15, 111, 38,
                    ]),
                },
                hash_merkle_root: SHA256([
                    227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39,
                    174, 65, 228, 100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85,
                ]),
            },
            proof: LeadershipProof {
                block_sig: Some(Secp256k1(Secp256k1Signature {
                    r: [
                        128, 205, 5, 48, 74, 223, 4, 72, 223, 231, 60, 90, 128, 196, 37, 74, 225,
                        60, 123, 112, 167, 2, 28, 201, 210, 41, 9, 128, 136, 223, 228, 35,
                    ],
                    s: [
                        128, 205, 5, 48, 74, 223, 4, 72, 223, 231, 60, 90, 128, 196, 37, 74, 225,
                        60, 123, 112, 167, 2, 28, 201, 210, 41, 9, 128, 136, 223, 228, 35,
                    ],
                    v: 0,
                })),
                influence: 0,
            },
            txns: vec![],
        });
        let raw_block = [146, 1, 145, 147, 147, 0, 146, 205, 1, 144, 146, 0, 145, 220, 0, 32, 47, 17, 204, 139, 204, 130, 7, 204, 164, 204, 151, 204, 185, 64, 43, 88, 204, 183, 53, 204, 213, 38, 89, 76, 66, 204, 231, 53, 78, 204, 216, 204, 230, 204, 217, 204, 245, 204, 184, 204, 150, 33, 204, 182, 15, 111, 38, 146, 0, 145, 220, 0, 32, 204, 227, 204, 176, 204, 196, 66, 204, 152, 204, 252, 28, 20, 204, 154, 204, 251, 204, 244, 204, 200, 204, 153, 111, 204, 185, 36, 39, 204, 174, 65, 204, 228, 100, 204, 155, 204, 147, 76, 204, 164, 204, 149, 204, 153, 27, 120, 82, 204, 184, 85, 146, 146, 0, 145, 147, 220, 0, 32, 204, 128, 204, 205, 5, 48, 74, 204, 223, 4, 72, 204, 223, 204, 231, 60, 90, 204, 128, 204, 196, 37, 74, 204, 225, 60, 123, 112, 204, 167, 2, 28, 204, 201, 204, 210, 41, 9, 204, 128, 204, 136, 204, 223, 204, 228, 35, 220, 0, 32, 204, 128, 204, 205, 5, 48, 74, 204, 223, 4, 72, 204, 223, 204, 231, 60, 90, 204, 128, 204, 196, 37, 74, 204, 225, 60, 123, 112, 204, 167, 2, 28, 204, 201, 204, 210, 41, 9, 204, 128, 204, 136, 204, 223, 204, 228, 35, 0, 0, 144];
        println!("{:?}", mined_block);
        println!("Mined block to bytes:");
        println!("{:?}", mined_block.to_bytes());
        println!("Mined block bytes from storage:");
        println!("{:?}", &raw_block[..]);
        assert_eq!(InventoryItem::from_bytes(&raw_block).unwrap(), mined_block);
        */
    }

    #[test]
    fn block_storable_fail() {
        use witnet_data_structures::chain::Hash::SHA256;
        use witnet_data_structures::chain::Signature::Secp256k1;
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let mined_block = InventoryItem::Block(Block {
            block_header: BlockHeader {
                version: 0,
                beacon: CheckpointBeacon {
                    checkpoint: 400,
                    hash_prev_block: SHA256([
                        47, 17, 139, 130, 7, 164, 151, 185, 64, 43, 88, 183, 53, 213, 38, 89, 76,
                        66, 231, 53, 78, 216, 230, 217, 245, 184, 150, 33, 182, 15, 111, 38,
                    ]),
                },
                hash_merkle_root: SHA256([
                    227, 176, 196, 66, 152, 252, 28, 20, 154, 251, 244, 200, 153, 111, 185, 36, 39,
                    174, 65, 228, 100, 155, 147, 76, 164, 149, 153, 27, 120, 82, 184, 85,
                ]),
            },
            proof: LeadershipProof {
                block_sig: Some(Secp256k1(Secp256k1Signature {
                    r: [
                        128, 205, 5, 48, 74, 223, 4, 72, 223, 231, 60, 90, 128, 196, 37, 74, 225,
                        60, 123, 112, 167, 2, 28, 201, 210, 41, 9, 128, 136, 223, 228, 35,
                    ],
                    s: [
                        128, 205, 5, 48, 74, 223, 4, 72, 223, 231, 60, 90, 128, 196, 37, 74, 225,
                        60, 123, 112, 167, 2, 28, 201, 210, 41, 9, 128, 136, 223, 228, 35,
                    ],
                    v: 0,
                })),
                influence: 0,
            },
            txns: vec![],
        });
        let msp = mined_block.to_bytes().unwrap();

        assert_eq!(InventoryItem::from_bytes(&msp).unwrap(), mined_block);
    }

    #[test]
    fn leadership_storable() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;
        let signed_beacon_hash = [4; 32];

        let signature = Signature::Secp256k1(Secp256k1Signature {
            r: signed_beacon_hash,
            s: signed_beacon_hash,
            v: 0,
        });
        let a = LeadershipProof {
            block_sig: Some(signature),
            influence: 0,
        };

        let msp = a.to_bytes().unwrap();

        assert_eq!(LeadershipProof::from_bytes(&msp).unwrap(), a);
    }

    #[test]
    fn signature_storable() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;
        let signed_beacon_hash = [4; 32];

        let a = Some(Signature::Secp256k1(Secp256k1Signature {
            r: signed_beacon_hash,
            s: signed_beacon_hash,
            v: 0,
        }));
        let msp = a.to_bytes().unwrap();

        assert_eq!(Option::<Signature>::from_bytes(&msp).unwrap(), a);
    }

    #[test]
    fn som_de_uno() {
        use witnet_storage::storage::Storable;

        let a = Some(Some(1u8));
        let msp = a.to_bytes().unwrap();
        assert_eq!(Option::<Option<u8>>::from_bytes(&msp).unwrap(), a);
    }

    #[test]
    fn empty_chain_state_to_bytes() {
        use witnet_storage::storage::Storable;

        let chain_state = ChainState::default();

        assert!(chain_state.to_bytes().is_ok());
    }

    #[test]
    fn chain_state_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let chain_state = ChainState {
            chain_info: Some(ChainInfo {
                environment: Environment::Mainnet,
                consensus_constants: ConsensusConstants {
                    checkpoint_zero_timestamp: 0,
                    checkpoints_period: 0,
                    genesis_hash: Hash::default(),
                    reputation_demurrage: 0.0,
                    reputation_punishment: 0.0,
                    max_block_weight: 0,
                },
                highest_block_checkpoint: CheckpointBeacon {
                    checkpoint: 0,
                    hash_prev_block: Hash::default(),
                },
            }),
            unspent_outputs_pool: UnspentOutputsPool::default(),
            data_request_pool: ActiveDataRequestPool::default(),
            block_chain: BTreeMap::default(),
        };

        assert!(chain_state.to_bytes().is_ok());
    }

    #[test]
    fn chain_state_with_chain_info_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let chain_state = ChainState {
            chain_info: Some(ChainInfo {
                environment: Environment::Testnet1,
                consensus_constants: ConsensusConstants {
                    checkpoint_zero_timestamp: 1546427376,
                    checkpoints_period: 10,
                    genesis_hash: Hash::SHA256([
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ]),
                    reputation_demurrage: 0.0,
                    reputation_punishment: 0.0,
                    max_block_weight: 10000,
                },
                highest_block_checkpoint: CheckpointBeacon {
                    checkpoint: 122533,
                    hash_prev_block: Hash::SHA256([
                        239, 173, 3, 247, 9, 44, 43, 68, 13, 51, 67, 110, 79, 191, 165, 135, 157,
                        167, 155, 126, 49, 39, 120, 119, 206, 75, 15, 74, 97, 167, 220, 214,
                    ]),
                },
            }),
            unspent_outputs_pool: UnspentOutputsPool::default(),
            data_request_pool: ActiveDataRequestPool::default(),
            block_chain: BTreeMap::default(),
        };

        assert!(chain_state.to_bytes().is_ok());
    }

    #[test]
    fn chain_state_with_utxo_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let mut utxo = UnspentOutputsPool::default();
        utxo.insert(
            OutputPointer {
                transaction_id: Hash::SHA256([
                    191, 75, 125, 95, 27, 78, 216, 89, 168, 222, 88, 21, 171, 139, 44, 170, 127,
                    120, 139, 142, 98, 209, 129, 129, 16, 52, 0, 62, 43, 116, 67, 245,
                ]),
                output_index: 0,
            },
            Output::ValueTransfer(ValueTransferOutput {
                pkh: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                value: 50000000000,
            }),
        );

        let chain_state = ChainState {
            chain_info: Some(ChainInfo {
                environment: Environment::Testnet1,
                consensus_constants: ConsensusConstants {
                    checkpoint_zero_timestamp: 1546427376,
                    checkpoints_period: 10,
                    genesis_hash: Hash::SHA256([
                        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                        0, 0, 0, 0, 0, 0, 0,
                    ]),
                    reputation_demurrage: 0.0,
                    reputation_punishment: 0.0,
                    max_block_weight: 10000,
                },
                highest_block_checkpoint: CheckpointBeacon {
                    checkpoint: 122533,
                    hash_prev_block: Hash::SHA256([
                        239, 173, 3, 247, 9, 44, 43, 68, 13, 51, 67, 110, 79, 191, 165, 135, 157,
                        167, 155, 126, 49, 39, 120, 119, 206, 75, 15, 74, 97, 167, 220, 214,
                    ]),
                },
            }),
            unspent_outputs_pool: utxo,
            data_request_pool: ActiveDataRequestPool::default(),
            block_chain: BTreeMap::default(),
        };

        assert!(chain_state.to_bytes().is_ok());
    }

    #[test]
    fn utxo_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let mut utxo = UnspentOutputsPool::default();
        utxo.insert(
            OutputPointer {
                transaction_id: Hash::SHA256([
                    191, 75, 125, 95, 27, 78, 216, 89, 168, 222, 88, 21, 171, 139, 44, 170, 127,
                    120, 139, 142, 98, 209, 129, 129, 16, 52, 0, 62, 43, 116, 67, 245,
                ]),
                output_index: 0,
            },
            Output::ValueTransfer(ValueTransferOutput {
                pkh: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                value: 50000000000,
            }),
        );

        assert!(utxo.to_bytes().is_ok());
    }

    #[test]
    fn output_pointer_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let output_pointer = OutputPointer {
            transaction_id: Hash::SHA256([
                191, 75, 125, 95, 27, 78, 216, 89, 168, 222, 88, 21, 171, 139, 44, 170, 127, 120,
                139, 142, 98, 209, 129, 129, 16, 52, 0, 62, 43, 116, 67, 245,
            ]),
            output_index: 0,
        };

        assert!(output_pointer.to_bytes().is_ok());
    }

    #[test]
    fn output_to_bytes() {
        use witnet_data_structures::chain::*;
        use witnet_storage::storage::Storable;

        let output = Output::ValueTransfer(ValueTransferOutput {
            pkh: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            value: 50000000000,
        });

        assert!(output.to_bytes().is_ok());
    }
}
