use super::SessionsManager;
use crate::config_mngr;
use actix::prelude::*;
use witnet_data_structures::chain::EpochConstants;
use witnet_util::timestamp::get_timestamp;

/// Make actor from `SessionsManager`
impl Actor for SessionsManager {
    /// Every actor has to provide execution `Context` in which it can run
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("Sessions Manager actor has been started!");

        // Send message to config manager and process its response
        config_mngr::get()
            .into_actor(self)
            .and_then(|config, act, ctx| {
                // Get periods for peers bootstrapping and discovery tasks
                let bootstrap_peers_period = config.connections.bootstrap_peers_period;
                let discovery_peers_period = config.connections.discovery_peers_period;
                let consensus_constants = config.consensus_constants.clone();

                // Set server address, connections limits and handshake timeout
                act.sessions
                    .set_public_address(config.connections.public_addr);
                act.sessions.set_limits(
                    config.connections.inbound_limit,
                    config.connections.outbound_limit,
                );
                act.sessions
                    .set_handshake_timeout(config.connections.handshake_timeout);
                act.sessions
                    .set_handshake_max_ts_diff(config.connections.handshake_max_ts_diff);
                act.sessions
                    .set_blocks_timeout(config.connections.blocks_timeout);

                // Initialized epoch from config
                let mut checkpoints_period = config.consensus_constants.checkpoints_period;
                let checkpoint_zero_timestamp =
                    config.consensus_constants.checkpoint_zero_timestamp;
                if checkpoints_period == 0 {
                    log::warn!("Setting the checkpoint period to the minimum value of 1 second");
                    checkpoints_period = 1;
                }
                let epoch_constants = EpochConstants {
                    checkpoint_zero_timestamp,
                    checkpoints_period,
                };
                act.current_epoch = epoch_constants
                    .epoch_at(get_timestamp())
                    .unwrap_or_default();

                act.sessions
                    .set_magic_number(consensus_constants.get_magic());

                // The peers bootstrapping process begins upon SessionsManager's start
                act.bootstrap_peers(ctx, bootstrap_peers_period);

                // The peers discovery process begins upon SessionsManager's start
                act.discovery_peers(ctx, discovery_peers_period);

                fut::ok(())
            })
            .map_err(|err, _, _| log::error!("Sessions manager startup error: {}", err))
            .wait(ctx);

        self.subscribe_to_epoch_manager(ctx);
    }
}
