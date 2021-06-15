use actix::{
    Actor, ActorContext, ActorFutureExt, AsyncContext, Context, ContextFutureSpawner, Running,
    SystemService, WrapFuture,
};

use witnet_data_structures::types::Message as WitnetMessage;
use witnet_p2p::sessions::{SessionStatus, SessionType};

use super::{handlers::EveryEpochPayload, Session};
use crate::actors::{
    chain_manager::ChainManager,
    epoch_manager::{EpochManager, EpochManagerError::CheckpointZeroInTheFuture},
    messages::{AddBlocks, GetEpoch, Register, Subscribe, Unregister},
    sessions_manager::SessionsManager,
};
use witnet_futures_utils::ActorFutureExt2;
use witnet_util::timestamp::pretty_print;

/// Implement actor trait for Session
impl Actor for Session {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        // Set Handshake timeout for stopping actor if session is still unconsolidated after given period of time
        ctx.run_later(self.config.connections.handshake_timeout, |act, ctx| {
            if act.status != SessionStatus::Consolidated {
                log::info!(
                    "Handshake timeout expired, disconnecting session with peer {:?}",
                    act.remote_addr
                );

                if act.session_type == SessionType::Outbound {
                    // Remove this address from tried bucket and ice it
                    act.remove_and_ice_peer();
                }

                ctx.stop();
            }
        });

        // Peer registered if it is not come from feeler
        if self.session_type == SessionType::Feeler {
            let version_msg = WitnetMessage::build_version(
                self.magic_number,
                self.public_addr,
                self.remote_addr,
                self.last_beacon.clone(),
            );
            self.send_message(version_msg);
            // Set HandshakeFlag of sent version message
            self.handshake_flags.version_tx = true;
        } else {
            self.subscribe_to_epoch_manager(ctx);

            // Get SessionsManager address
            let sessions_manager_addr = SessionsManager::from_registry();

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
                .then(|res, act, ctx| match res {
                    Ok(Ok(_)) => {
                        log::trace!(
                            "Successfully registered session {:?} into SessionManager",
                            act.remote_addr
                        );

                        actix::fut::ok(())
                    }
                    _ => {
                        log::error!("Session register into Session Manager failed");
                        ctx.stop();

                        actix::fut::err(())
                    }
                })
                .and_then(|_, act, _ctx| {
                    // Send version if outbound session
                    if let SessionType::Outbound = act.session_type {
                        let version_msg = WitnetMessage::build_version(
                            act.magic_number,
                            act.public_addr,
                            act.remote_addr,
                            act.last_beacon.clone(),
                        );
                        act.send_message(version_msg);
                        // Set HandshakeFlag of sent version message
                        act.handshake_flags.version_tx = true;
                    }

                    actix::fut::ok(())
                })
                .map(|_res: Result<(), ()>, _act, _ctx| ())
                .wait(ctx);
        }
    }

    /// Method to be executed when the actor is stopping
    fn stopping(&mut self, _: &mut Self::Context) -> Running {
        // Get session manager address
        let session_manager_addr = SessionsManager::from_registry();

        // Unregister session from SessionsManager
        session_manager_addr.do_send(Unregister {
            address: self.remote_addr,
            session_type: self.session_type,
            status: self.status,
        });

        // When session unregisters, notify ChainManager to stop waiting for new blocks
        if self.blocks_timestamp != 0 {
            // Get ChainManager address
            let chain_manager_addr = ChainManager::from_registry();

            chain_manager_addr.do_send(AddBlocks {
                blocks: vec![],
                sender: None,
            });
            log::warn!("Session disconnected during block exchange");
        }

        Running::Stop
    }
}

impl Session {
    /// Get epoch from EpochManager and subscribe to future epochs
    fn subscribe_to_epoch_manager(&mut self, ctx: &mut Context<Session>) {
        // Get EpochManager address from registry
        let epoch_manager_addr = EpochManager::from_registry();

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
                        act.current_epoch = epoch;
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
}
