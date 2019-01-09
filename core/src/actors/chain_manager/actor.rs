use actix::{Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, System, WrapFuture};

use crate::actors::epoch_manager::{
    messages::{GetEpoch, Subscribe},
    EpochManager,
    EpochManagerError::CheckpointZeroInTheFuture,
};

use super::{handlers::*, mining::MiningNotification, ChainManager};

use crate::actors::{
    config_manager::send_get_config_request,
    storage_keys::CHAIN_KEY,
    storage_manager::{messages::Get, StorageManager},
};

use witnet_data_structures::chain::{ChainInfo, CheckpointBeacon};

use witnet_util::timestamp::{get_timestamp, pretty_print};

use log::{debug, error, warn};

/// Implement Actor trait for `ChainManager`
impl Actor for ChainManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("ChainManager actor has been started!");

        // Use the current timestamp as a random value to modify the signature
        // Make sure to wait at least 1 second before starting each node
        self.random = get_timestamp() as u64;

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

        // Query ConfigManager for initial configuration and process response
        send_get_config_request(self, ctx, |act, ctx, config| {
            // Get environment and consensus_constants parameters from config
            let environment = (&config.environment).clone();
            let consensus_constants = (&config.consensus_constants).clone();

            act.max_block_weight = consensus_constants.max_block_weight;

            // Get storage manager actor address
            let storage_manager_addr = System::current().registry().get::<StorageManager>();
            storage_manager_addr
                // Send a message to read the chain_info from the storage
                .send(Get::<ChainInfo>::new(CHAIN_KEY))
                .into_actor(act)
                // Process the response
                .then(|res, _act, _ctx| match res {
                    Err(e) => {
                        // Error when sending message
                        error!("Unsuccessful communication with storage manager: {}", e);
                        actix::fut::err(())
                    }
                    Ok(res) => match res {
                        Err(e) => {
                            // Storage error
                            error!("Error while getting ChainInfo from storage: {}", e);
                            actix::fut::err(())
                        }
                        Ok(res) => actix::fut::ok(res),
                    },
                })
                .and_then(move |chain_info_from_storage, act, _ctx| {
                    // chain_info_from_storage can be None if the storage does not contain that key
                    if let Some(chain_info_from_storage) = chain_info_from_storage {
                        if environment == chain_info_from_storage.environment {
                            if consensus_constants == chain_info_from_storage.consensus_constants {
                                // Update Chain Info from storage
                                let chain_info = ChainInfo {
                                    environment,
                                    consensus_constants,
                                    highest_block_checkpoint: chain_info_from_storage
                                        .highest_block_checkpoint,
                                };
                                act.chain_info = Some(chain_info);
                                debug!("ChainInfo successfully obtained from storage");
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
                        act.chain_info = Some(chain_info);
                    }
                    actix::fut::ok(())
                })
                .wait(ctx);

            // Do not start the MiningManager if the configuration disables it
            if config.mining.enabled {
                debug!("MiningManager actor has been started!");

                // Subscribe to epoch manager
                // Get EpochManager address from registry
                let epoch_manager_addr = System::current().registry().get::<EpochManager>();

                // Subscribe to all epochs with an EveryEpochPayload
                epoch_manager_addr.do_send(Subscribe::to_all(ctx.address(), MiningNotification));
            } else {
                debug!("MiningManager explicitly disabled by configuration.");
            }
        });
    }
}
