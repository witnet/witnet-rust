use actix::prelude::*;
use std::{cmp, collections::HashMap};
use web3::{ethabi::Bytes, types::U256};
use witnet_data_structures::chain::Hash;

/// EthPoller (TODO: Explanation)
#[derive(Default)]
pub struct DrDatabase {
    dr: HashMap<DrId, DrState>,
    max_dr_id: DrId,
}

/// Data request ID, as set in the ethereum contract
pub type DrId = U256;

/// Data request state
pub enum DrState {
    /// New: the data request has just been posted to the ethereum contract.
    New {
        /// Data Request bytes
        dr_bytes: Bytes,
    },
    /// Pending: the data request has been created and broadcasted to witnet, but it has not been
    /// included in a witnet block yet.
    Pending {
        /// Data request transaction hash
        dr_tx_hash: Hash,
    },
    /// Finished: data request has been resolved in witnet and the result is in the ethreum
    /// contract.
    Finished,
}

/// Make actor from DrDatabase
impl Actor for DrDatabase {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("DrReporter actor has been started!");
    }
}

/// Set data request state
pub struct SetDrState(pub DrId, pub DrState);

impl Message for SetDrState {
    type Result = ();
}

/// Get a list of all the data requests in "pending" state
pub struct GetAllPendingDrs;

impl Message for GetAllPendingDrs {
    type Result = Result<Vec<(DrId, Hash)>, ()>;
}

/// Get the highest data request id from the database
pub struct GetLastDrId;

impl Message for GetLastDrId {
    type Result = Result<DrId, ()>;
}

impl Handler<SetDrState> for DrDatabase {
    type Result = ();

    fn handle(&mut self, msg: SetDrState, _ctx: &mut Self::Context) -> Self::Result {
        let SetDrState(dr_id, dr_state) = msg;
        self.dr.insert(dr_id, dr_state);

        self.max_dr_id = cmp::max(self.max_dr_id, dr_id);
    }
}

impl Handler<GetAllPendingDrs> for DrDatabase {
    type Result = Result<Vec<(DrId, Hash)>, ()>;

    fn handle(&mut self, _msg: GetAllPendingDrs, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self
            .dr
            .iter()
            .filter_map(|(dr_id, dr_state)| {
                if let DrState::Pending { dr_tx_hash } = dr_state {
                    Some((*dr_id, *dr_tx_hash))
                } else {
                    None
                }
            })
            .collect())
    }
}

impl Handler<GetLastDrId> for DrDatabase {
    type Result = Result<DrId, ()>;

    fn handle(&mut self, _msg: GetLastDrId, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self.max_dr_id)
    }
}

/// Required trait for being able to retrieve DrDatabase address from system registry
impl actix::Supervised for DrDatabase {}

/// Required trait for being able to retrieve DrDatabase address from system registry
impl SystemService for DrDatabase {}
