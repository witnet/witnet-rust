use actix::prelude::*;

use super::PeersManager;
use crate::{
    actors::{
        epoch_manager::EpochManager,
        messages::{GetEpoch, Subscribe},
        storage_keys,
    },
    config_mngr, storage_mngr,
};
use witnet_p2p::peers::Peers;

/// Make actor from PeersManager
impl Actor for PeersManager {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("Peers Manager actor has been started!");

        // Send message to config manager and process response
        config_mngr::get()
            .into_actor(self)
            .and_then(|config, mut act, ctx| {
                // Get known peers
                let known_peers: Vec<_> = config.connections.known_peers.iter().cloned().collect();

                // Get connection periods from config
                let storage_peers_period = config.connections.storage_peers_period;
                act.bucketing_update_period = config.connections.bucketing_update_period;
                let feeler_peers_period = config.connections.feeler_peers_period;
                let check_melted_peers_period = config.connections.check_melted_peers_period;

                // Add all peers
                log::info!(
                    "Adding the following peer addresses from config: {:?}",
                    known_peers
                );
                match act.peers.add_to_new(known_peers.clone(), None) {
                    Ok(_duplicated_peers) => {}
                    Err(e) => log::error!("Error when adding peer addresses from config: {}", e),
                }

                let consensus_constants = (&config.consensus_constants).clone();
                let magic = consensus_constants.get_magic();
                act.set_magic(magic);

                storage_mngr::get::<_, Peers>(&storage_keys::peers_key(magic))
                    .into_actor(act)
                    .map_err(|e, _, _| log::error!("Couldn't get peers from storage: {}", e))
                    .and_then(move |peers_from_storage, act, _ctx| {
                        // peers_from_storage can be None if the storage does not contain that key
                        if let Some(peers_from_storage) = peers_from_storage {
                            // Add all the peers from storage
                            // The add method handles duplicates by overwriting the old values
                            act.import_peers(peers_from_storage, known_peers);
                        }

                        fut::ok(())
                    })
                    .spawn(ctx);

                // Ask EpochManager for current epoch so that `Peers` knows about the bootstrapping
                // status. If there is no current epoch, subscribe to first epoch so that the
                // `bootstrapped` flag can be later set to `true` once actually bootstrapped.
                let epoch_manager = EpochManager::from_registry();
                epoch_manager
                    .send(GetEpoch)
                    .into_actor(act)
                    .then(move |epoch, act, ctx| {
                        if epoch.is_ok() {
                            act.peers.bootstrapped = true
                        } else {
                            epoch_manager
                                .send(Subscribe::to_epoch(0, ctx.address(), ()))
                                .into_actor(act)
                                .then(|_, _, _| fut::ok(()))
                                .spawn(ctx);
                        }

                        fut::ok(())
                    })
                    .spawn(ctx);

                // Start the storage peers process on PeersManager start
                act.persist_peers(ctx, storage_peers_period);

                // The peers melt process begins upon PeersManager's start
                act.melt_peers(ctx, check_melted_peers_period);

                // Start the feeleer peers process on PeersManager start
                act.feeler(ctx, feeler_peers_period);

                fut::ok(())
            })
            .map_err(|err, _, _| log::error!("Peer discovery failed: {}", err))
            .wait(ctx);
    }
}
