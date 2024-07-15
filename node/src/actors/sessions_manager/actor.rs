use super::SessionsManager;
use crate::config_mngr;
use actix::prelude::*;
use witnet_data_structures::{
    chain::EpochConstants, get_protocol_version_activation_epoch, get_protocol_version_period,
    proto::versioning::ProtocolVersion,
};

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
                // Store a reference to config
                act.config = Some(config.clone());

                // Get periods for peers bootstrapping and discovery tasks
                let bootstrap_peers_period = config.connections.bootstrap_peers_period;
                let discovery_peers_period = config.connections.discovery_peers_period;

                // Set server address, connections limits, handshake timeout and optional features
                act.sessions
                    .set_public_address(config.connections.public_addr);
                act.sessions.set_limits(
                    config.connections.inbound_limit,
                    config.connections.outbound_limit,
                );

                // Set reject_sybil_outbounds range limit
                act.sessions
                    .inbound_network_ranges
                    .set_range_limit(config.connections.reject_sybil_inbounds_range_limit);

                // Initialized epoch from config
                let checkpoints_period = config.consensus_constants.checkpoints_period;
                let checkpoint_zero_timestamp =
                    config.consensus_constants.checkpoint_zero_timestamp;
                let checkpoint_zero_timestamp_v2 = checkpoint_zero_timestamp
                    + get_protocol_version_activation_epoch(ProtocolVersion::V2_0) as i64
                        * checkpoints_period as i64;
                let checkpoints_period_v2 = get_protocol_version_period(ProtocolVersion::V2_0);
                let epoch_constants = EpochConstants {
                    checkpoint_zero_timestamp,
                    checkpoints_period,
                    checkpoint_zero_timestamp_v2,
                    checkpoints_period_v2,
                };
                act.current_epoch = epoch_constants
                    .epoch_at(get_timestamp())
                    .unwrap_or_default();

                act.sessions.set_magic_number(10700u16);

                // The peers bootstrapping process begins upon SessionsManager's start
                act.bootstrap_peers(ctx, bootstrap_peers_period);

                // The peers discovery process begins upon SessionsManager's start
                act.discovery_peers(ctx, discovery_peers_period);

                fut::ok(())
            })
            .map_err(|err, _, _| log::error!("Sessions manager startup error: {}", err))
            .map(|_res: Result<(), ()>, _act, _ctx| ())
            .wait(ctx);

        self.subscribe_to_epoch_manager(ctx);
    }
}
