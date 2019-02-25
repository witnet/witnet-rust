use actix::{
    Actor, ActorContext, ActorFuture, AsyncContext, Context, ContextFutureSpawner, Running, System,
    WrapFuture,
};
use log::{debug, error, info};

use witnet_data_structures::types::Message as WitnetMessage;
use witnet_p2p::sessions::{SessionStatus, SessionType};

use super::Session;
use crate::actors::{
    messages::{Register, Unregister},
    sessions_manager::SessionsManager,
};

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

        Running::Stop
    }
}
