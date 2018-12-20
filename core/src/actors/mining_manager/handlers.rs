use actix::{ActorFuture, Context, ContextFutureSpawner, Handler, System, WrapFuture};

use super::MiningManager;
use crate::actors::chain_manager::messages::GetHighestCheckpointBeacon;
use crate::actors::chain_manager::ChainManager;
use crate::actors::epoch_manager::messages::EpochNotification;

use witnet_crypto::hash::calculate_sha256;
use witnet_data_structures::chain::{Hash, LeadershipProof, Secp256k1Signature, Signature};
use witnet_storage::storage::Storable;

use crate::actors::chain_manager::messages::BuildBlock;
use crate::actors::reputation_manager::messages::ValidatePoE;
use crate::actors::reputation_manager::ReputationManager;
use log::{debug, error, info};

////////////////////////////////////////////////////////////////////////////////////////
// ACTOR MESSAGE HANDLERS
////////////////////////////////////////////////////////////////////////////////////////

/// Payload for the notification for all epochs
#[derive(Clone, Debug)]
pub struct EveryEpochPayload;

/// Handler for EpochNotification<EveryEpochPayload>
impl Handler<EpochNotification<EveryEpochPayload>> for MiningManager {
    type Result = ();

    fn handle(&mut self, msg: EpochNotification<EveryEpochPayload>, ctx: &mut Context<Self>) {
        debug!("Periodic epoch notification received {:?}", msg.checkpoint);

        let chain_manager_addr = System::current().registry().get::<ChainManager>();
        chain_manager_addr
            .send(GetHighestCheckpointBeacon)
            .into_actor(self)
            .then(move |beacon_msg, act, ctx| {
                // Check eligibility
                // S(H(beacon))
                let mut beacon = match beacon_msg {
                    Ok(Ok(b)) => b,
                    _ => return actix::fut::err(()),
                };
                if beacon.checkpoint > msg.checkpoint {
                    // We got a block from the future
                    error!(
                        "The current highest checkpoint beacon is from the future ({:?} > {:?})",
                        beacon.checkpoint, msg.checkpoint
                    );
                    return actix::fut::err(());
                }
                if beacon.checkpoint == msg.checkpoint {
                    // For some reason we already got a valid block for this epoch
                    // TODO: Check eligibility anyway?
                }
                // The highest checkpoint beacon should contain the current epoch
                beacon.checkpoint = msg.checkpoint;
                let beacon_hash = Hash::from(calculate_sha256(&beacon.to_bytes().unwrap()));
                let private_key = 1;

                // TODO: send Sign message to CryptoManager
                let sign = |x, _k| match x {
                    Hash::SHA256(mut x) => {
                        // Add some randomness to the signature
                        x[0] = act.random as u8;
                        x
                    }
                };
                let signed_beacon_hash = sign(beacon_hash, private_key);
                // Currently, every hash is valid
                // Fake signature which will be accepted anyway
                let signature = Signature::Secp256k1(Secp256k1Signature {
                    r: signed_beacon_hash,
                    s: signed_beacon_hash,
                    v: 0,
                });
                let leadership_proof = LeadershipProof {
                    block_sig: Some(signature),
                    influence: 0,
                };

                // Send ValidatePoE message to ReputationManager
                let reputation_manager_addr =
                    System::current().registry().get::<ReputationManager>();
                reputation_manager_addr
                    .send(ValidatePoE {
                        beacon,
                        proof: leadership_proof,
                    })
                    .into_actor(act)
                    .drop_err()
                    .and_then(move |eligible, _act, _ctx| {
                        if eligible {
                            info!(
                                "Discovered eligibility for mining a block for epoch #{:?}",
                                beacon.checkpoint
                            );
                            // Send proof of eligibility to chain manager,
                            // which will construct and broadcast the block
                            chain_manager_addr.do_send(BuildBlock {
                                beacon,
                                leadership_proof,
                            });
                        }
                        actix::fut::ok(())
                    })
                    .wait(ctx);
                actix::fut::ok(())
            })
            .wait(ctx);
    }
}
