use actix::{Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, System, WrapFuture};

use crate::actors::epoch_manager::{
    messages::{GetEpoch, Subscribe},
    Epoch, EpochManager,
};

use crate::actors::blocks_manager::{
    handlers::{EpochPayload, EveryEpochPayload},
    BlocksManager,
};

use crate::actors::{
    config_manager::send_get_config_request,
    storage_keys::CHAIN_KEY,
    storage_manager::{messages::Get, StorageManager},
};

use witnet_data_structures::chain::{ChainInfo, Checkpoint};

use log::{debug, error, info};

/// Implement Actor trait for `BlocksManager`
impl Actor for BlocksManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Blocks Manager actor has been started!");

        // TODO begin remove this once BlocksManager real functionality is implemented
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
            .then(move |res, _act, ctx| {
                // Get BlocksManager address
                let blocks_manager_addr = ctx.address();

                // Check GetEpoch result
                match res {
                    Ok(Ok(epoch)) => {
                        // Subscribe to the next epoch with an EpochPayload
                        epoch_manager_addr.do_send(Subscribe::to_epoch(
                            Epoch(epoch.0 + 1),
                            blocks_manager_addr.clone(),
                            EpochPayload,
                        ));

                        // Subscribe to all epochs with an EveryEpochPayload
                        epoch_manager_addr
                            .do_send(Subscribe::to_all(blocks_manager_addr, EveryEpochPayload));
                    }
                    _ => {
                        error!("Current epoch could not be retrieved from EpochManager");
                    }
                }

                actix::fut::ok(())
            })
            .wait(ctx);
        // TODO end remove this once blocks manager real functionality is implemented

        // Query ConfigManager for initial configuration and process response
        send_get_config_request(self, ctx, |act, ctx, config| {
            // Get environment and consensus_constants parameters from config
            let environment = (&config.environment).clone();
            let consensus_constants = (&config.consensus_constants).clone();

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
                            error!("Error while getting chain info from storage: {}", e);
                            actix::fut::err(())
                        }
                        Ok(res) => actix::fut::ok(res),
                    },
                })
                .and_then(move |chain_info_from_storage, act, _ctx| {
                    // chain_info_from_storage can be None if the storage does not contain that key
                    if let Some(chain_info_from_storage) = chain_info_from_storage {
                        if (environment == chain_info_from_storage.environment)
                            & (consensus_constants == chain_info_from_storage.consensus_constants)
                        {
                            // Update Chain Info from storage
                            let chain_info = ChainInfo {
                                environment,
                                consensus_constants,
                                highest_block_checkpoint: chain_info_from_storage
                                    .highest_block_checkpoint,
                            };
                            act.chain_info = Some(chain_info);
                            info!("ChainInfo successfully obtained from storage");
                        } else {
                            // There are differences in protocol constants and/or environment
                            // between storage and config
                            panic!("Error protocol constants and/or environment are different");
                        }
                    } else {
                        debug!("Not ChainInfo in storage, proceeding to create it");
                        // Create a new ChainInfo
                        let genesis_hash = (consensus_constants.genesis_hash).clone();
                        let chain_info = ChainInfo {
                            environment,
                            consensus_constants,
                            highest_block_checkpoint: Checkpoint {
                                number: 0,
                                hash: genesis_hash,
                            },
                        };
                        act.chain_info = Some(chain_info);
                    }
                    actix::fut::ok(())
                })
                .wait(ctx);

            // Persist chain_info into storage
            act.persist_chain_info(ctx);
        });
    }
}
