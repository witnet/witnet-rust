use actix::prelude::*;
use std::{pin::Pin, str::FromStr, time::Duration};

use super::{handlers::EveryEpochPayload, ChainManager};
use crate::{
    actors::{
        epoch_manager::{EpochManager, EpochManagerError::CheckpointZeroInTheFuture},
        messages::{AddBlocks, GetEpoch, GetEpochConstants, SetLastBeacon, Subscribe},
        sessions_manager::SessionsManager,
        storage_keys,
    },
    config_mngr, signature_mngr, storage_mngr,
};
use witnet_crypto::key::CryptoEngine;
use witnet_data_structures::{
    chain::{
        ChainInfo, ChainState, CheckpointBeacon, CheckpointVRF, GenesisBlockInfo, PublicKeyHash,
        ReputationEngine,
    },
    data_request::DataRequestPool,
    get_environment,
    superblock::SuperBlockState,
    types::LastBeacon,
    utxo_pool::OwnUnspentOutputsPool,
    vrf::VrfCtx,
};
use witnet_storage::storage::WriteBatch;
use witnet_util::timestamp::pretty_print;

/// Implement Actor trait for `ChainManager`
impl Actor for ChainManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("ChainManager actor has been started!");

        ctx.wait(Self::check_only_one_chain_state_in_storage().into_actor(self));

        self.initialize_from_storage(ctx);

        self.subscribe_to_epoch_manager(ctx);

        self.get_pkh(ctx);

        self.get_bn256_public_key(ctx);

        self.vrf_ctx = VrfCtx::secp256k1()
            .map_err(|e| {
                log::error!("Failed to create VRF context: {}", e);
                // Stop the node
                ctx.stop();
            })
            .ok();

        self.secp = Some(CryptoEngine::new());
    }
}

impl ChainManager {
    /// Get configuration from ConfigManager and try to initialize ChainManager state from Storage
    /// (initialize to Default values if empty)
    pub fn initialize_from_storage(&mut self, ctx: &mut Context<ChainManager>) {
        let fut = self
            .initialize_from_storage_fut(false)
            .map(|_res, _act, _ctx| ());
        ctx.wait(fut);
    }

    /// Get configuration from ConfigManager and try to initialize ChainManager state from Storage
    /// (initialize to Default values if empty)
    pub fn initialize_from_storage_fut(
        &mut self,
        resync: bool,
    ) -> ResponseActFuture<Self, Result<(), ()>> {
        let fut = config_mngr::get()
            .into_actor(self)
            .map_err(|err, _act, _ctx| {
                log::error!("Couldn't get config: {}", err);
            })
            .and_then(|config, act, _ctx| {
                let consensus_constants = config.consensus_constants.clone();

                if config.mining.data_request_timeout == Duration::new(0, 0) {
                    act.data_request_timeout = None;
                } else {
                    act.data_request_timeout = Some(config.mining.data_request_timeout);
                }

                // Set the retrievals limit per epoch, as read from the configuration
                act.data_request_max_retrievals_per_epoch = config.mining.data_request_max_retrievals_per_epoch;

                act.tx_pending_timeout = config.mempool.tx_pending_timeout;

                let magic = consensus_constants.get_magic();
                act.set_magic(magic);

                // Do not start the MiningManager if the configuration disables it
                act.mining_enabled = config.mining.enabled;

                // Set the maximum reinserted transaction number
                act.max_reinserted_transactions = config.mempool.max_reinserted_transactions as usize;

                // External mint address
                act.external_address = config.mining.mint_external_address.clone().and_then(|pkh| PublicKeyHash::from_str(pkh.as_str()).ok());
                // External mint percentage should not exceed 100%
                act.external_percentage = std::cmp::min(config.mining.mint_external_percentage, 100);

                // Get consensus parameter from config
                act.consensus_c = config.connections.consensus_c;

                act.chain_state_snapshot.superblock_period = consensus_constants.superblock_period;

                // Set weight limit of transactions pool
                let vt_to_dr_factor = f64::from(config.consensus_constants.max_vt_weight) / f64::from(config.consensus_constants.max_dr_weight);
                let _removed_transactions = act.transactions_pool.set_total_weight_limit(config.mining.transactions_pool_total_weight_limit, vt_to_dr_factor);

                // Minimum fee required to include a VTT into a block
                act.transactions_pool.set_minimum_vtt_fee(config.mining.minimum_vtt_fee);

                // Store settings for Threshold Activation of Protocol Improvements
                act.tapi = config.tapi.clone();

                storage_mngr::get_chain_state(&storage_keys::chain_state_key(magic))
                    .into_actor(act)
                    .then(|chain_state_from_storage, _, _| {
                        let result = match chain_state_from_storage {
                            Ok(x) => (x, config),
                            Err(e) => {
                                panic!("Error while getting chain state from storage: {}", e);
                            }
                        };

                        actix::fut::ok(result)
                    })
            })
            .map_ok(move |(chain_state_from_storage, config), _act, _ctx| {
                // Get environment and consensus_constants parameters from config
                let environment = config.environment;
                let consensus_constants = &config.consensus_constants;
                // chain_info_from_storage can be None if the storage does not contain that key

                let chain_state = match chain_state_from_storage {
                    Some(
                        chain_state_from_storage @ ChainState {
                            chain_info: Some(..),
                            reputation_engine: Some(..),
                            ..
                        }
                    ) => {
                        let chain_info_from_storage =
                            chain_state_from_storage.chain_info.as_ref().unwrap();

                        if environment == chain_info_from_storage.environment {
                            if consensus_constants == &chain_info_from_storage.consensus_constants {
                                log::debug!("ChainInfo successfully obtained from storage");

                                chain_state_from_storage
                            } else {
                                // Mismatching consensus constants between config and storage
                                panic!(
                                    "Mismatching consensus constants: tried to run a node using \
                                     different consensus constants than the ones that were used when \
                                     the local chain was initialized.\nNode constants: {:#?}\nChain \
                                     constants: {:#?}",
                                    consensus_constants, chain_info_from_storage.consensus_constants
                                );
                            }
                        } else {
                            // Mismatching environment names between config and storage
                            panic!(
                                "Mismatching environments: tried to run a node on environment \
                                \"{:?}\" with a chain that was initialized with environment \
                                \"{:?}\".",
                                environment, chain_info_from_storage.environment
                            );
                        }
                    }
                    x => {
                        if x.is_some() {
                            log::debug!(
                                "Uninitialized local chain the ChainInfo in storage is incomplete. Proceeding to \
                                 initialize and store a new chain."
                            );
                        } else {
                            log::debug!(
                                "Uninitialized local chain (no ChainInfo in storage). Proceeding to \
                                 initialize and store a new chain."
                            );
                        }
                        // Create a new ChainInfo
                        let bootstrap_hash = consensus_constants.bootstrap_hash;
                        let reputation_engine = ReputationEngine::new(consensus_constants.activity_period as usize);
                        let hash_prev_block = bootstrap_hash;

                        let chain_info = ChainInfo {
                            environment,
                            consensus_constants: consensus_constants.clone(),
                            highest_block_checkpoint: CheckpointBeacon {
                                checkpoint: 0,
                                hash_prev_block,
                            },
                            highest_superblock_checkpoint: CheckpointBeacon {
                                checkpoint: 0,
                                hash_prev_block,
                            },
                            highest_vrf_output: CheckpointVRF {
                                checkpoint: 0,
                                hash_prev_vrf: hash_prev_block,
                            },
                        };

                        let bootstrap_committee = chain_info
                            .consensus_constants
                            .bootstrapping_committee
                            .iter()
                            .map(|add| add.parse().expect("Malformed bootstrapping committee"))
                            .collect();
                        let superblock_state = SuperBlockState::new(bootstrap_hash, bootstrap_committee);

                        ChainState {
                            chain_info: Some(chain_info),
                            reputation_engine: Some(reputation_engine),
                            own_utxos: OwnUnspentOutputsPool::new(),
                            data_request_pool: DataRequestPool::new(consensus_constants.extra_rounds),
                            superblock_state,
                            ..ChainState::default()
                        }
                    }
                };

                (chain_state, config)
            })
            .and_then(move |(chain_state, config), act, _ctx| {
                // Get storage backend for unspent_outputs_pool.
                // Avoid call to storage_mngr if the backend is already in memory.
                let fut: Pin<Box<dyn ActorFuture<Self, Output = Result<_, ()>>>> = if let Some(x) = act.chain_state.unspent_outputs_pool.db.take() {
                    Box::pin(actix::fut::ok(x))
                } else {
                    Box::pin(storage_mngr::get_backend()
                        .into_actor(act)
                        .map_err(|err, _act, _ctx| {
                            log::error!("Failed to get storage backend: {}", err);
                        }))
                };

                fut.map_ok(move |backend, _act, _ctx| (chain_state, config, backend))
            })
            .and_then(move |(mut chain_state, config, backend), act, _ctx| {
                chain_state.unspent_outputs_pool.db = Some(backend);

                let fut: Pin<Box<dyn ActorFuture<Self, Output = Result<ChainState, ()>>>> = if !chain_state.unspent_outputs_pool_old_migration_db.is_empty() {
                    log::info!("Detected some UTXOs stored in memory, performing migration to store all UTXOs in database");
                    // In case a previous attempt to perform this migration was interrupted, remove
                    // all the existing UTXOs from database first.
                    let removed_utxos = chain_state.unspent_outputs_pool.delete_all_from_db();
                    if removed_utxos > 0 {
                        log::warn!(
                            "Found {} UTXOs already in the database. Assuming that this is \
                            because a previous migration was interrupted, the UTXOs have \
                            been deleted and the migration will restart from scratch.",
                            removed_utxos
                        );
                    }
                    chain_state
                        .unspent_outputs_pool
                        .migrate_old_unspent_outputs_pool_to_db(
                            &mut chain_state.unspent_outputs_pool_old_migration_db,
                            |i, total| {
                                if i % 10000 == 0 {
                                    log::info!(
                                        "UTXO set migration v3: [{}/{}]",
                                        i,
                                        total,
                                    );
                                }
                            }
                        );
                    log::info!("Migration completed successfully, saving updated ChainState");
                    // Write the chain state again right after this migration, to ensure that the
                    // migration is only executed once
                    let fut = storage_mngr::put_chain_state_in_batch(
                        &storage_keys::chain_state_key(act.get_magic()),
                        &chain_state,
                        WriteBatch::default(),
                    )
                        .into_actor(act)
                        .and_then(|_, _, _| {
                            log::debug!("Successfully persisted chain_state into storage");
                            fut::ok(chain_state)
                        })
                        .map_err(|err, _, _| {
                            log::error!(
                    "Failed to persist chain_state into storage: {}",
                    err
                )
                        });

                    Box::pin(fut)
                } else {
                    Box::pin(actix::fut::ok(chain_state))
                };

                fut
                .map_ok(move |chain_state, _act, _ctx| {
                    (chain_state, config)
                })
            })
            .map_ok(move |(chain_state, config), act, ctx| {
                let consensus_constants = &config.consensus_constants;
                let chain_info = chain_state.chain_info.as_ref().unwrap();
                log::info!(
                    "Actual ChainState CheckpointBeacon: epoch ({}), hash_block ({})",
                    chain_info.highest_block_checkpoint.checkpoint,
                    chain_info.highest_block_checkpoint.hash_prev_block
                );

                // If hash_prev_block is the bootstrap hash, create and consolidate genesis block.
                // Consolidating the genesis block is not needed if the chain state has been reset
                // because of a rewind: the genesis block will be processed with the other blocks.
                if !resync && chain_info.highest_block_checkpoint.hash_prev_block == consensus_constants.bootstrap_hash {
                    let info_genesis =
                        GenesisBlockInfo::from_path(&config.mining.genesis_path, consensus_constants.bootstrap_hash, consensus_constants.genesis_hash)
                            .map_err(|e| {
                                log::error!("Failed to create genesis block: {}", e);
                                log::error!("Genesis block could be downloaded in: https://github.com/witnet/genesis_block");
                                System::current().stop_with_code(1);
                            }).ok();

                    if let Some(ig) = info_genesis {
                        log::info!("Genesis block successfully created. Hash: {}", consensus_constants.genesis_hash);

                        let genesis_block = ig.build_genesis_block(consensus_constants.bootstrap_hash);
                        ctx.notify(AddBlocks {
                            blocks: vec![genesis_block],
                            sender: None,
                        });
                    }
                }

                act.chain_state = chain_state;

                // Update possible new WIP information
                let (new_wip_epoch, old_wips) = act.chain_state.tapi_engine.initialize_wip_information(get_environment());
                let last_consolidated_epoch = act.get_chain_beacon().checkpoint;
                if new_wip_epoch < last_consolidated_epoch {
                    // Some blocks have been consolidated before this node updated to the latest version,
                    // so we need to count the missing wip_votes from that blocks
                    ctx.wait(
                        act.update_new_wip_votes(new_wip_epoch, last_consolidated_epoch, old_wips)
                            .map(|_res, _act, _ctx| ())
                    );
                }

                // initialize_from_storage is also used to implement reorganizations
                // In that case, we must clear some fields to avoid forks
                act.best_candidate = None;
                act.candidates.clear();
                act.seen_candidates.clear();

                // clean transactions included during an unconfirmed superepoch
                let mempool_transactions = act.transactions_pool.remove_unconfirmed_transactions();
                act.temp_vts_and_drs.extend(mempool_transactions);
                if !act.temp_vts_and_drs.is_empty(){
                    log::debug!(
                        "Re-adding {} transactions into mempool",
                        act.temp_vts_and_drs.len(),
                    );
                }

                // Delete any saved copies of the old chain state to avoid accidentally persisting
                // a forked state
                act.chain_state_snapshot.clear();
                // When initializing for the first time, we need to set the
                // highest_persisted_superblock to the top consolidated superblock
                act.chain_state_snapshot.highest_persisted_superblock = act.get_superblock_beacon().checkpoint;

                SessionsManager::from_registry().do_send(SetLastBeacon {
                    beacon: LastBeacon {
                        highest_block_checkpoint: act.get_chain_beacon(),
                        highest_superblock_checkpoint: act.get_superblock_beacon(),
                    },
                });

            });

        Box::pin(fut)
    }

    /// Ensure that there is only one ChainState in the storage. Old versions of witnet-rust used
    /// to allow having multiple ChainStates for multiple testnets. This function will panic in that
    /// case because multiple ChainStates are incompatible with storing UTXOs as keys in the
    /// database.
    pub async fn check_only_one_chain_state_in_storage() {
        let config = config_mngr::get().await.unwrap_or_else(|err| {
            panic!("Couldn't get config: {}", err);
        });

        let backend = storage_mngr::get_backend().await.unwrap_or_else(|err| {
            panic!("Failed to get storage backend: {}", err);
        });

        let magic = config.consensus_constants.get_magic();

        // The key prefix depends on the length of the number when converted to string, so we need
        // to check all the possible lengths for numbers between 0 and 65535.
        // Luckily there are only 5 possible prefixes:
        let magic_templates = vec![1, 11, 111, 1111, 11111];
        let all_chain_states: Vec<Vec<u8>> = magic_templates
            .into_iter()
            .map(storage_keys::chain_state_key)
            .map(|key| bincode::serialize(&key).expect("bincode error"))
            .map(|mut key_bytes| {
                // Truncate key to 14 bytes. If the key is:
                // [8 bytes prefix]chain-11111-key
                // This will truncate right before the "11111":
                // [8 bytes prefix]chain-
                // To allow checking all the possible magic numbers
                key_bytes.truncate(14);
                key_bytes
            })
            .flat_map(|prefix| {
                backend
                    .prefix_iterator(&prefix)
                    .expect("prefix iterator error")
                    .map(|(k, _v)| k)
                    .collect::<Vec<_>>()
            })
            .collect();

        match all_chain_states.len() {
            0 => {
                // No ChainState in DB, good
            }
            1 => {
                let expected_magic = storage_keys::chain_state_key(magic);
                let expected_key = bincode::serialize(&expected_magic).expect("bincode error");
                let key = &all_chain_states[0];
                if key == &expected_key {
                    // One ChainState in DB and matches magic number, good
                } else {
                    // One ChainState in DB but does not match magic number, bad.
                    // We would need to delete existing chain state and all utxos to be able to
                    // reuse this storage, so ask the user to delete the storage.
                    let key_str: String =
                        bincode::deserialize(key).unwrap_or_else(|_e| format!("{:?}", key));
                    panic!(
                        "Storage already contains a chain state with different magic number.\n\
                        Expected magic: {:?}, found in storage: {:?}.\n\
                        Please backup the master key if needed, delete the storage and try again.\n\
                        To backup the master key, you need to go back to the previous environment \
                        (testnet or mainnet) and use the exportMasterKey command.\n\
                        Help: make sure that the storage folder is not used in multiple environments \
                        (testnet and mainnet).",
                        expected_magic, key_str
                    );
                }
            }
            _ => {
                // More than one ChainState in DB, bad.
                // This could lead to UTXOs from one environment being spent on a different
                // environment, so ask the user to delete the storage.
                let expected_magic = storage_keys::chain_state_key(magic);
                let key_strs: Vec<String> = all_chain_states
                    .iter()
                    .map(|key| bincode::deserialize(key).unwrap_or_else(|_e| format!("{:?}", key)))
                    .collect();
                panic!(
                    "Storage contains more than one chain state with different magic numbers.\n\
                    Expected magic: {:?}, found in storage: {:?}.\n\
                    Please backup the master key if needed, delete the storage and try again.\n\
                    To backup the master key, downgrade to witnet 1.4.3 and use the exportMasterKey command.\n\
                    Help: make sure that the storage folder is not used in multiple environments \
                    (testnet and mainnet).",
                    expected_magic, key_strs
                );
            }
        }
    }

    /// Get epoch constants and current epoch from EpochManager, and subscribe to future epochs
    fn subscribe_to_epoch_manager(&mut self, ctx: &mut Context<ChainManager>) {
        // Get EpochManager address from registry
        let epoch_manager_addr = EpochManager::from_registry();
        let epoch_manager_addr2 = epoch_manager_addr.clone();

        // Get epoch constants
        epoch_manager_addr.send(GetEpochConstants).into_actor(self).then(move |res, act, _ctx| {
            match res {
                Ok(f) => act.epoch_constants = f,
                error => log::error!("Failed to get epoch constants: {:?}", error),
            }

            epoch_manager_addr2
                // Send GetEpoch message to epoch manager actor
                // This returns a RequestFuture, representing an asynchronous message sending process
                .send(GetEpoch)
                // Convert a normal future into an ActorFuture
                .into_actor(act)
        })
            // Process the response from the EpochManager
            // This returns a FutureResult containing the socket address if present
            .then(move |res, act, ctx| {
                // Get ChainManager address
                let chain_manager_addr = ctx.address();

                // Check GetEpoch result
                match res {
                    Ok(Ok(epoch)) => {
                        // Subscribe to all epochs with an EveryEpochPayload
                        epoch_manager_addr
                            .do_send(Subscribe::to_all(chain_manager_addr, EveryEpochPayload));

                        // Set current_epoch
                        act.current_epoch = Some(epoch);
                    }
                    Ok(Err(CheckpointZeroInTheFuture(zero))) => {
                        let date = pretty_print(zero, 0);
                        log::warn!("Network bootstrapping is scheduled for {:?}. The node will remain idle and delay chain bootstrapping until then. Wait for it!", date);

                        // Subscribe to all epochs with an EveryEpochPayload
                        epoch_manager_addr
                            .do_send(Subscribe::to_all(chain_manager_addr, EveryEpochPayload));
                    }
                    error => {
                        log::error!("Current epoch could not be retrieved from EpochManager: {:?}", error);
                    }
                }

                actix::fut::ready(())
            })
            .wait(ctx);
    }

    /// Load public key hash from signature manager
    fn get_pkh(&mut self, ctx: &mut Context<Self>) {
        signature_mngr::pkh()
            .into_actor(self)
            .map_err(|e, _act, _ctx| {
                log::error!(
                    "Error while getting public key hash from signature manager: {}",
                    e
                );
            })
            .and_then(|res, act, _ctx| {
                act.own_pkh = Some(res);
                log::debug!("Address successfully loaded from signature manager");
                log::info!("Node identity / address: {}", res);

                // Telemetry is configured here to avoid a race condition if configured directly
                // upon actor start because of asynchronous nature of `get_key` function.
                act.configure_telemetry_scope();

                actix::fut::ok(())
            })
            .map(|_res: Result<(), ()>, _act, _ctx| ())
            .wait(ctx);
    }

    /// Load bn256 public key from signature manager
    fn get_bn256_public_key(&mut self, ctx: &mut Context<Self>) {
        signature_mngr::bn256_public_key()
            .into_actor(self)
            .map_err(|e, _act, _ctx| {
                log::error!(
                    "Error while getting bn256 public key from signature manager: {}",
                    e
                );
            })
            .and_then(|res, act, _ctx| {
                act.bn256_public_key = Some(res);
                log::debug!("Bn256 public key successfully loaded from signature manager");
                actix::fut::ok(())
            })
            .map(|_res: Result<(), ()>, _act, _ctx| ())
            .wait(ctx);
    }

    /// Put some basic information into the scope of the telemetry service (Sentry)
    #[cfg(feature = "telemetry")]
    fn configure_telemetry_scope(&mut self) {
        log::debug!("Configuring telemetry scope");
        sentry::configure_scope(|scope| {
            scope.set_user(self.own_pkh.map(|address| sentry::User {
                id: Some(address.bech32(witnet_data_structures::get_environment())),
                ..Default::default()
            }))
        });
    }
    #[cfg(not(feature = "telemetry"))]
    fn configure_telemetry_scope(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actors::storage_keys::chain_state_key, utils::test_actix_system};
    use std::sync::Arc;
    use witnet_config::config::{Config, StorageBackend};

    #[test]
    fn test_check_only_one_chain_state_in_storage_empty_storage() {
        let _ = env_logger::builder().is_test(true).try_init();
        test_actix_system(|| async {
            // Setup testing: use in-memory database instead of rocksdb
            let mut config = Config::default();
            config.storage.backend = StorageBackend::HashMap;
            let config = Arc::new(config);
            // Start relevant actors
            config_mngr::start(config);
            storage_mngr::start();

            ChainManager::check_only_one_chain_state_in_storage().await;
        });
    }

    #[test]
    fn test_check_only_one_chain_state_in_storage_one_ok() {
        let _ = env_logger::builder().is_test(true).try_init();
        test_actix_system(|| async {
            // Setup testing: use in-memory database instead of rocksdb
            let mut config = Config::default();
            config.storage.backend = StorageBackend::HashMap;
            let config = Arc::new(config);
            let magic = config.consensus_constants.get_magic();
            // Start relevant actors
            config_mngr::start(config);
            storage_mngr::start();

            let chain_manager = ChainManager::default();
            storage_mngr::put_chain_state_in_batch(
                &storage_keys::chain_state_key(magic),
                &chain_manager.chain_state,
                WriteBatch::default(),
            )
            .await
            .expect("failed to store chain state");

            ChainManager::check_only_one_chain_state_in_storage().await;
        });
    }

    #[test]
    #[should_panic = "Storage already contains a chain state with different magic number"]
    fn test_check_only_one_chain_state_in_storage_one_different() {
        let _ = env_logger::builder().is_test(true).try_init();
        test_actix_system(|| async {
            // Setup testing: use in-memory database instead of rocksdb
            let mut config = Config::default();
            config.storage.backend = StorageBackend::HashMap;
            let config = Arc::new(config);
            let magic = config.consensus_constants.get_magic();
            // Start relevant actors
            config_mngr::start(config);
            storage_mngr::start();

            let chain_manager = ChainManager::default();
            // Change one bit of the magic number to trigger the error
            let magic = magic ^ 0x01;
            storage_mngr::put_chain_state_in_batch(
                &storage_keys::chain_state_key(magic),
                &chain_manager.chain_state,
                WriteBatch::default(),
            )
            .await
            .expect("failed to store chain state");

            ChainManager::check_only_one_chain_state_in_storage().await;
        });
    }

    #[test]
    #[should_panic = "Storage contains more than one chain state with different magic numbers"]
    fn test_check_only_one_chain_state_in_storage_two_chain_states() {
        let _ = env_logger::builder().is_test(true).try_init();
        test_actix_system(|| async {
            // Setup testing: use in-memory database instead of rocksdb
            let mut config = Config::default();
            config.storage.backend = StorageBackend::HashMap;
            let config = Arc::new(config);
            // Start relevant actors
            config_mngr::start(config);
            storage_mngr::start();

            let chain_manager = ChainManager::default();
            let magic1 = 1;
            let magic2 = 2;
            storage_mngr::put_chain_state_in_batch(
                &chain_state_key(magic1),
                &chain_manager.chain_state,
                WriteBatch::default(),
            )
            .await
            .expect("failed to store chain state");
            storage_mngr::put_chain_state_in_batch(
                &chain_state_key(magic2),
                &chain_manager.chain_state,
                WriteBatch::default(),
            )
            .await
            .expect("failed to store chain state");

            ChainManager::check_only_one_chain_state_in_storage().await;
        });
    }
}
