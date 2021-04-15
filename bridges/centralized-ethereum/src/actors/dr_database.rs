use actix::prelude::*;
use std::{cmp, collections::HashMap, fmt};
use web3::{ethabi::Bytes, types::U256};
use witnet_data_structures::chain::Hash;

/// Dr Database actor handles the states of the different requests read from Ethereum
#[derive(Default)]
pub struct DrDatabase {
    dr: HashMap<DrId, DrInfoBridge>,
    max_dr_id: DrId,
}

/// Data request ID, as set in the ethereum contract
pub type DrId = U256;

/// Data Request Information for the Bridge
pub struct DrInfoBridge {
    /// Data Request Bytes
    pub dr_bytes: Bytes,
    /// Data Request State
    pub dr_state: DrState,
    /// Data Request Transaction Hash
    pub dr_tx_hash: Option<Hash>,
}

/// Data request state
#[derive(Clone)]
pub enum DrState {
    /// New: the data request has just been posted to the ethereum contract.
    New,
    /// Pending: the data request has been created and broadcasted to witnet, but it has not been
    /// included in a witnet block yet.
    Pending,
    /// Finished: data request has been resolved in witnet and the result is in the ethreum
    /// contract.
    Finished,
}

impl fmt::Display for DrState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DrState::New => "New",
            DrState::Pending => "Pending",
            DrState::Finished => "Finished",
        };

        f.write_str(s)
    }
}

/// Make actor from DrDatabase
impl Actor for DrDatabase {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("DRDataBase actor has been started!");
    }
}

/// Set data request state
pub struct SetDrInfoBridge(pub DrId, pub DrInfoBridge);

impl Message for SetDrInfoBridge {
    type Result = ();
}

/// Get a list of all the data requests in "new" state
pub struct GetAllNewDrs;

impl Message for GetAllNewDrs {
    type Result = Result<Vec<(DrId, Bytes)>, ()>;
}

/// Get a list of all the data requests in "pending" state
pub struct GetAllPendingDrs;

impl Message for GetAllPendingDrs {
    type Result = Result<Vec<(DrId, Bytes, Hash)>, ()>;
}

/// Get the highest data request id from the database
pub struct GetLastDrId;

impl Message for GetLastDrId {
    type Result = Result<DrId, ()>;
}

impl Handler<SetDrInfoBridge> for DrDatabase {
    type Result = ();

    fn handle(&mut self, msg: SetDrInfoBridge, _ctx: &mut Self::Context) -> Self::Result {
        let SetDrInfoBridge(dr_id, dr_info) = msg;
        let dr_state = dr_info.dr_state.clone();
        self.dr.insert(dr_id, dr_info);

        self.max_dr_id = cmp::max(self.max_dr_id, dr_id);
        log::debug!("Data request #{} inserted with state {}", dr_id, dr_state);
    }
}

impl Handler<GetAllNewDrs> for DrDatabase {
    type Result = Result<Vec<(DrId, Bytes)>, ()>;

    fn handle(&mut self, _msg: GetAllNewDrs, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self
            .dr
            .iter()
            .filter_map(|(dr_id, dr_info)| {
                if let DrState::New = dr_info.dr_state {
                    Some((*dr_id, dr_info.dr_bytes.clone()))
                } else {
                    None
                }
            })
            .collect())
    }
}

impl Handler<GetAllPendingDrs> for DrDatabase {
    type Result = Result<Vec<(DrId, Bytes, Hash)>, ()>;

    fn handle(&mut self, _msg: GetAllPendingDrs, _ctx: &mut Self::Context) -> Self::Result {
        Ok(self
            .dr
            .iter()
            .filter_map(|(dr_id, dr_info)| {
                if let DrState::Pending = dr_info.dr_state {
                    Some((
                        *dr_id,
                        dr_info.dr_bytes.clone(),
                        dr_info.dr_tx_hash.unwrap(),
                    ))
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
