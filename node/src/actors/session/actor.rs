use actix::{
    Actor, ActorContext, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Running, System,
    WrapFuture,
};
use log::{debug, error, info, warn};

use witnet_data_structures::types::Message as WitnetMessage;
use witnet_p2p::sessions::{SessionStatus, SessionType};

use super::{handlers::EveryEpochPayload, Session};
use crate::actors::{
    chain_manager::ChainManager,
    epoch_manager::{EpochManager, EpochManagerError::CheckpointZeroInTheFuture},
    messages::{AddBlocks, GetEpoch, Register, Subscribe, Unregister},
    sessions_manager::SessionsManager,
};
use witnet_util::timestamp::pretty_print;

/// Implement actor trait for Session
impl Actor for Session {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        // Set Handshake timeout for stopping actor if session is still unconsolidated after given period of time
        ctx.run_later(self.handshake_timeout, |act, ctx| {
            if act.status != SessionStatus::Consolidated {
                info!(
                    "Handshake timeout expired, disconnecting session with peer {:?}",
                    act.remote_addr
                );
                if let SessionStatus::Unconsolidated = act.status {
                    ctx.stop();
                }
            }
        });

        self.subscribe_to_epoch_manager(ctx);

        // Get SessionsManager address
        let sessions_manager_addr = System::current().registry().get::<SessionsManager>();

        // Register self in SessionsManager. `AsyncContext::wait` register
        // future within context, but context waits until this future resolves
        // before processing any other events.
        sessions_manager_addr
            .send(Register {
                address: self.remote_addr,
                actor: ctx.address(),
                session_type: self.session_type,
            })
            .into_actor(self)
            .then(|res, act, ctx| {
                match res {
                    Ok(Ok(_)) => {
                        debug!(
                            "Successfully registered session {:?} into SessionManager",
                            act.remote_addr
                        );

                        actix::fut::ok(())
                    }
                    _ => {
                        error!("Session register into Session Manager failed");
                        // FIXME(#72): a full stop of the session is not correct (unregister should
                        // be skipped)
                        ctx.stop();

                        actix::fut::err(())
                    }
                }
            })
            .and_then(|_, act, _ctx| {
                // Send version if outbound session
                if let SessionType::Outbound = act.session_type {
                    // FIXME(#142): include the checkpoint of the current tip of the local blockchain
                    let version_msg = WitnetMessage::build_version(
                        act.magic_number,
                        act.server_addr,
                        act.remote_addr,
                        0,
                    );
                    act.send_message(version_msg);
                    // Set HandshakeFlag of sent version message
                    act.handshake_flags.version_tx = true;
                }

                actix::fut::ok(())
            })
            .wait(ctx);
    }

    /// Method to be executed when the actor is stopping
    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // Get session manager address
        let session_manager_addr = System::current().registry().get::<SessionsManager>();

        // Unregister session from SessionsManager
        session_manager_addr.do_send(Unregister {
            address: self.remote_addr,
            session_type: self.session_type,
            status: self.status,
        });

        // When session unregisters, notify ChainManager to stop waiting for new blocks
        if self.blocks_timestamp != 0 {
            // Get ChainManager address
            let chain_manager_addr = System::current().registry().get::<ChainManager>();

            chain_manager_addr.do_send(AddBlocks { blocks: vec![] });
            warn!("Session disconnected during block exchange");
        }

        Running::Stop
    }
}

impl Session {
    /// Get epoch from EpochManager and subscribe to future epochs
    fn subscribe_to_epoch_manager(&mut self, ctx: &mut Context<Session>) {
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
                            .do_send(Subscribe::to_epoch(0, chain_manager_addr, EveryEpochPayload))
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
