use actix::prelude::*;
use log::{debug, error, info};

use super::PeersManager;
use crate::actors::storage_keys::PEERS_KEY;
use crate::config_mngr;
use crate::storage_mngr;
use witnet_p2p::peers::Peers;

/// Make actor from PeersManager
impl Actor for PeersManager {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        debug!("Peers Manager actor has been started!");

        // Send message to config manager and process response
        config_mngr::get()
            .into_actor(self)
            .and_then(|config, act, ctx| {
                // Get known peers
                let known_peers: Vec<_> = config.connections.known_peers.iter().cloned().collect();

                // Get storage peers period
                let storage_peers_period = config.connections.storage_peers_period;

                // Add all peers
                info!(
                    "Adding the following peer addresses from config: {:?}",
                    known_peers
                );
                match act.peers.add(known_peers) {
                    Ok(_duplicated_peers) => {}
                    Err(e) => error!("Error when adding peer addresses from config: {}", e),
                }

                storage_mngr::get::<_, Peers>(&PEERS_KEY)
                    .into_actor(act)
                    .map_err(|e, _, _| error!("Couldn't get peers from storage: {}", e))
                    .and_then(|peers_from_storage, act, _| {
                        // peers_from_storage can be None if the storage does not contain that key
                        if let Some(peers_from_storage) = peers_from_storage {
                            // Add all the peers from storage
                            // The add method handles duplicates by overwriting the old values
                            let peers = peers_from_storage.get_all().unwrap();
                            info!(
                                "Adding the following peer addresses from storage: {:?}",
                                peers
                            );
                            match act.peers.add(peers) {
                                Ok(_duplicated_peers) => {}
                                Err(e) => {
                                    error!("Error when adding peer addresses from storage: {}", e);
                                }
                            }
                        }

                        fut::ok(())
                    })
                    .spawn(ctx);

                // Start the storage peers process on SessionsManager start
                act.persist_peers(ctx, storage_peers_period);

                fut::ok(())
            })
            .map_err(|err, _, _| log::error!("Peer discovery failed: {}", err))
            .wait(ctx);
    }
}
