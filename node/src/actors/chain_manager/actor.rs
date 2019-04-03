// use actix::{Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, System, WrapFuture};
use actix::prelude::*;

use super::{
    handlers::{EpochPayload, EveryEpochPayload},
    ChainManager,
};
use crate::actors::{
    epoch_manager::{EpochManager, EpochManagerError::CheckpointZeroInTheFuture},
    messages::{GetEpoch, Subscribe},
    storage_keys::CHAIN_STATE_KEY,
};
use crate::config_mngr;
use crate::storage_mngr;
use witnet_data_structures::{
    chain::{Blockchain, ChainInfo, ChainState, CheckpointBeacon, UnspentOutputsPool},
    data_request::DataRequestPool,
};

use witnet_util::timestamp::{get_timestamp_nanos, pretty_print};

use log::{debug, error, warn};

/// Implement Actor trait for `ChainManager`
impl Actor for ChainManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    // FIXME: Remove all `unwrap()`s
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("ChainManager actor has been started!");

        // Use the current timestamp as a random value to modify the signature
        // Make sure to wait at least 1 second before starting each node
        self.random = u64::from(get_timestamp_nanos().1);

        self.initialize_from_storage(ctx);

        self.subscribe_to_epoch_manager(ctx);
    }
}

impl ChainManager {
    /// Get configuration from ConfigManager and try to initialize ChainManager state from Storage
    /// (initialize to Default values if empty)
    fn initialize_from_storage(&mut self, ctx: &mut Context<ChainManager>) {
        config_mngr::get().into_actor(self).and_then(|config, act, ctx| {
            // Get environment and consensus_constants parameters from config
            let environment = (&config.environment).clone();
            let consensus_constants = (&config.consensus_constants).clone();

            act.max_block_weight = consensus_constants.max_block_weight;

            storage_mngr::get::<_, ChainState>(&CHAIN_STATE_KEY)
                .into_actor(act)
                .map_err(|e, _, _| error!("Error while getting chain state from storage: {}", e))
                .and_then(move |chain_state_from_storage, act, _ctx| {
                    // chain_info_from_storage can be None if the storage does not contain that key
                    if chain_state_from_storage.is_some()
                        && chain_state_from_storage
                        .as_ref()
                        .unwrap()
                        .chain_info
                        .is_some()
                    {
                        let chain_state_from_storage = chain_state_from_storage.unwrap();
                        let chain_info_from_storage =
                            chain_state_from_storage.chain_info.as_ref().unwrap();

                        if environment == chain_info_from_storage.environment {
                            if consensus_constants == chain_info_from_storage.consensus_constants {
                                // Update Chain Info from storage
                                act.chain_state = chain_state_from_storage;
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
                    } else {
                        debug!(
                            "Uninitialized local chain (no ChainInfo in storage). Proceeding to \
                             initialize and store a new chain."
                        );
                        // Create a new ChainInfo
                        let genesis_hash = consensus_constants.genesis_hash;
                        let chain_info = ChainInfo {
                            environment,
                            consensus_constants,
                            highest_block_checkpoint: CheckpointBeacon {
                                checkpoint: 0,
                                hash_prev_block: genesis_hash,
                            },
                        };
                        act.chain_state = ChainState {
                            chain_info: Some(chain_info),
                            unspent_outputs_pool: UnspentOutputsPool::default(),
                            data_request_pool: DataRequestPool::default(),
                            block_chain: Blockchain::default(),
                        };
                    }

                    fut::ok(())
                })
                .spawn(ctx);

            // Store the genesis block hash
            act.genesis_block_hash = config.consensus_constants.genesis_hash;

            // Do not start the MiningManager if the configuration disables it
            act.mining_enabled = config.mining.enabled;

            if act.mining_enabled {
                debug!("Mining enabled!");
            } else {
                debug!("Mining explicitly disabled by configuration.");
            }

            fut::ok(())
        }).map_err(|err,_,_| {
            log::error!("Couldn't initialize from storage: {}", err);
        }).wait(ctx);
    }

    /// Get epoch from EpochManager and subscribe to future epochs
    fn subscribe_to_epoch_manager(&mut self, ctx: &mut Context<ChainManager>) {
        // Get EpochManager address from registry
        let epoch_manager_addr = System::current().registry().get::<EpochManager>();

        // Start chain of actions
        epoch_manager_addr
            // Send GetEpoch message to epoch manager actor
            // This returns a RequestFuture, representing an asynchronous message sending process
            .send(GetEpoch)
            // Convert a normal future into an ActorFuture
            .into_actor(self)
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
}
