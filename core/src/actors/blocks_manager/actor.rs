use actix::{Actor, ActorFuture, AsyncContext, Context, ContextFutureSpawner, System, WrapFuture};

use crate::actors::epoch_manager::{
    messages::{GetEpoch, Subscribe},
    Epoch, EpochManager,
};

use crate::actors::blocks_manager::{
    handlers::{EpochMessage, PeriodicMessage},
    BlocksManager,
};

use log::{debug, error};

/// Make actor from `BlocksManager`
impl Actor for BlocksManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Blocks Manager actor has been started!");

        // TODO begin remove this once blocks manager real functionality is implemented
        // Get EpochManager address from registry
        let epoch_manager_addr = System::current().registry().get::<EpochManager>();

        // Start chain of actions
        epoch_manager_addr
            // Send GetEpoch message to epoch manager actor
            // This returns a Request Future, representing an asynchronous message sending process
            .send(GetEpoch)
            // Convert a normal future into an ActorFuture
            .into_actor(self)
            // Process the response from the epoch manager
            // This returns a FutureResult containing the socket address if present
            .then(move |res, _act, ctx| {
                // Get blocks manager address
                let blocks_manager_addr = ctx.address();

                // Check GetEpoch result
                match res {
                    Ok(Ok(epoch)) => {
                        // Subscribe to the next epoch with a EpochMessage
                        epoch_manager_addr.do_send(Subscribe::to_epoch(
                            Epoch(epoch.0 + 1),
                            blocks_manager_addr.clone(),
                            EpochMessage,
                        ));

                        // Subscribe to all epochs with a PeriodicMessage
                        epoch_manager_addr
                            .do_send(Subscribe::to_all(blocks_manager_addr, PeriodicMessage));
                    }
                    _ => {
                        error!("Current epoch could not be retrieved from EpochManager");
                    }
                }

                actix::fut::ok(())
            })
            .wait(ctx);
        // TODO end remove this once blocks manager real functionality is implemented
    }
}
