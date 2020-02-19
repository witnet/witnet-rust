// use actix::{Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, System, WrapFuture};
use actix::prelude::*;

use super::{
    handlers::{EpochPayload, EveryEpochPayload},
    ChainManager,
};
use crate::{
    actors::{
        epoch_manager::{EpochManager, EpochManagerError::CheckpointZeroInTheFuture},
        messages::{GetEpoch, GetEpochConstants, Subscribe},
        storage_keys,
    },
    config_mngr, signature_mngr, storage_mngr,
};
use witnet_data_structures::{
    chain::{ChainInfo, ChainState, CheckpointBeacon, ReputationEngine},
    vrf::VrfCtx,
};

use witnet_util::timestamp::pretty_print;

use log::{debug, error, info, warn};
use std::time::Duration;
use witnet_crypto::key::CryptoEngine;

/// Implement Actor trait for `ChainManager`
impl Actor for ChainManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("ChainManager actor has been started!");

        self.initialize_from_storage(ctx);

        self.subscribe_to_epoch_manager(ctx);

        self.get_pkh(ctx);

        self.vrf_ctx = VrfCtx::secp256k1()
            .map_err(|e| {
                error!("Failed to create VRF context: {}", e);
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
    // FIXME: Remove all `unwrap()`s
    pub fn initialize_from_storage(&mut self, ctx: &mut Context<ChainManager>) {
        config_mngr::get().into_actor(self).and_then(|config, act, ctx| {
            // Get environment and consensus_constants parameters from config
            let environment = config.environment;

            let consensus_constants = config.consensus_constants.clone();
            act.max_block_weight = consensus_constants.max_block_weight;
            if config.mining.data_request_timeout == Duration::new(0, 0) {
                act.data_request_timeout = None;
            } else {
                act.data_request_timeout = Some(config.mining.data_request_timeout);
            }

            act.tx_pending_timeout = config.mempool.tx_pending_timeout;

            let magic = consensus_constants.get_magic();
            act.set_magic(magic);

            storage_mngr::get::<_, ChainState>(&storage_keys::chain_state_key(magic))
                .into_actor(act)
                .map_err(|e, _, _| error!("Error while getting chain state from storage: {}", e))
                .and_then(move |chain_state_from_storage, act, _ctx| {
                    // chain_info_from_storage can be None if the storage does not contain that key
                    match chain_state_from_storage {
                        Some(
                            ChainState {
                                chain_info: Some(..),
                                reputation_engine: Some(..),
                                ..
                            }
                        ) => {
                            let chain_state_from_storage = chain_state_from_storage.unwrap();
                            let chain_info_from_storage =
                                chain_state_from_storage.chain_info.as_ref().unwrap();

                            if environment == chain_info_from_storage.environment {
                                if consensus_constants == chain_info_from_storage.consensus_constants {
                                    // Update Chain Info from storage
                                    act.chain_state = chain_state_from_storage;
                                    act.last_chain_state = act.chain_state.clone();
                                    debug!("ChainInfo successfully obtained from storage");
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
                                debug!(
                                    "Uninitialized local chain the ChainInfo in storage is incomplete. Proceeding to \
                                     initialize and store a new chain."
                                );
                            } else {
                                debug!(
                                    "Uninitialized local chain (no ChainInfo in storage). Proceeding to \
                                     initialize and store a new chain."
                                );
                            }
                            // Create a new ChainInfo
                            let bootstrap_hash = consensus_constants.bootstrap_hash;
                            let reputation_engine = ReputationEngine::new(consensus_constants.activity_period as usize);
                            let chain_info = ChainInfo {
                                environment,
                                consensus_constants,
                                highest_block_checkpoint: CheckpointBeacon {
                                    checkpoint: 0,
                                    hash_prev_block: bootstrap_hash,
                                },
                            };
                            act.chain_state = ChainState {
                                chain_info: Some(chain_info),
                                reputation_engine: Some(reputation_engine),
                                ..ChainState::default()
                            };
                            act.last_chain_state = act.chain_state.clone();
                        }
                    }

                    let chain_info = act.chain_state.chain_info.as_ref().unwrap();
                    info!("Actual ChainState CheckpointBeacon: epoch ({}), hash_block ({})",
                          chain_info.highest_block_checkpoint.checkpoint,
                          chain_info.highest_block_checkpoint.hash_prev_block);

                    fut::ok(())
                })
                .spawn(ctx);

            // Store the bootstrap block hash
            act.bootstrap_block_hash = config.consensus_constants.bootstrap_hash;

            // Do not start the MiningManager if the configuration disables it
            act.mining_enabled = config.mining.enabled;

            // Get consensus parameter from config
            act.consensus_c = config.connections.consensus_c;

            fut::ok(())
        }).map_err(|err,_,_| {
            log::error!("Couldn't initialize from storage: {}", err);
        }).wait(ctx);
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
                error => error!("Failed to get epoch constants: {:?}", error),
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
                        warn!("Checkpoint zero is in the future ({:?}). Delaying chain bootstrapping until then.", date);
                        // Subscribe to first epoch
                        epoch_manager_addr
                            .do_send(Subscribe::to_epoch(0, chain_manager_addr, EpochPayload))
                    }
                    error => {
                        error!("Current epoch could not be retrieved from EpochManager: {:?}", error);
                    }
                }

                actix::fut::ok(())
            })
            .wait(ctx);
    }

    /// Load public key hash from signature manager
    fn get_pkh(&mut self, ctx: &mut Context<Self>) {
        signature_mngr::pkh()
            .into_actor(self)
            .map_err(|e, _act, _ctx| {
                error!(
                    "Error while getting public key hash from signature manager: {}",
                    e
                );
            })
            .and_then(|res, act, _ctx| {
                act.own_pkh = Some(res);
                debug!("Public key hash successfully loaded from signature manager");
                info!("PublicKeyHash: {}", res);
                actix::fut::ok(())
            })
            .wait(ctx);
    }
}
