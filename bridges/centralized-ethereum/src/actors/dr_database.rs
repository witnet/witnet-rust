use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::{cmp, collections::HashMap, fmt};
use web3::ethabi::Bytes;
use web3::types::U256;
use witnet_data_structures::chain::Hash;
use witnet_node::storage_mngr;
use witnet_node::utils::stop_system_if_panicking;

/// Database key that stores the Data Request information
const BRIDGE_DB_KEY: &[u8] = b"bridge_db_key";

/// Dr Database actor handles the states of the different requests read from Ethereum
#[derive(Default, Serialize, Deserialize, Clone)]
pub struct DrDatabase {
    dr: HashMap<DrId, DrInfoBridge>,
    max_dr_id: DrId,
}

impl Drop for DrDatabase {
    fn drop(&mut self) {
        log::trace!("Dropping DrDatabase");
        stop_system_if_panicking("DrDatabase");
    }
}

/// Data request ID, as set in the ethereum contract
pub type DrId = U256;

/// Data Request Information for the Bridge
#[derive(Default, Serialize, Deserialize, Clone)]
pub struct DrInfoBridge {
    /// Data Request Bytes
    pub dr_bytes: Bytes,
    /// Data Request State
    pub dr_state: DrState,
    /// Data Request Transaction Hash
    pub dr_tx_hash: Option<Hash>,
    /// Data Request Transaction creation date
    pub dr_tx_creation_timestamp: Option<i64>,
}

/// Data request state
#[derive(Serialize, Deserialize, Clone)]
pub enum DrState {
    /// New: the data request has just been posted to the smart contract.
    New,
    /// Pending: the data request has been created and broadcast to witnet, but it has not been
    /// included in a witnet block yet.
    Pending,
    /// Finished: data request has been resolved in witnet and the result is in the smart
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

impl Default for DrState {
    fn default() -> Self {
        Self::New
    }
}

/// Data request states in Witnet Request Board contract
#[derive(Serialize, Deserialize, Clone)]
pub enum WitnetQueryStatus {
    /// Unknown: the data request does not exist.
    Unknown,
    /// Posted: the data request has just been posted to the smart contract.
    Posted,
    /// Reported: the data request has been resolved in witnet and the result is in the smart
    /// contract.
    Reported,
    /// Deleted: the data request has been resolved in witnet but the result was deleted.
    Deleted,
}

impl WitnetQueryStatus {
    /// Maps uint8 to WitnetQueryStatus enum
    pub fn from_code(i: u8) -> Self {
        match i {
            1 => WitnetQueryStatus::Posted,
            2 => WitnetQueryStatus::Reported,
            3 => WitnetQueryStatus::Deleted,
            _ => WitnetQueryStatus::Unknown,
        }
    }
}

/// Make actor from DrDatabase
impl Actor for DrDatabase {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("DrDatabase actor has been started!");

        let fut = storage_mngr::get::<_, DrDatabase>(&BRIDGE_DB_KEY)
            .into_actor(self)
            .map(
                |dr_database_from_storage, act, _| match dr_database_from_storage {
                    Ok(dr_database_from_storage) => {
                        if let Some(mut dr_database_from_storage) = dr_database_from_storage {
                            log::info!("Load database from storage");
                            act.dr = std::mem::take(&mut dr_database_from_storage.dr);
                            act.max_dr_id = dr_database_from_storage.max_dr_id;
                        } else {
                            log::info!("No database in storage");
                        }
                    }
                    Err(e) => {
                        panic!("Error while getting bridge database from storage: {}", e);
                    }
                },
            );

        ctx.wait(fut);
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
    type Result = Result<Vec<(DrId, Bytes, Hash, i64)>, ()>;
}

/// Get the highest data request id from the database
pub struct GetLastDrId;

impl Message for GetLastDrId {
    type Result = Result<DrId, ()>;
}

impl Handler<SetDrInfoBridge> for DrDatabase {
    type Result = ();

    fn handle(&mut self, msg: SetDrInfoBridge, ctx: &mut Self::Context) -> Self::Result {
        let SetDrInfoBridge(dr_id, dr_info) = msg;
        let dr_state = dr_info.dr_state.clone();
        self.dr.insert(dr_id, dr_info);

        self.max_dr_id = cmp::max(self.max_dr_id, dr_id);
        log::debug!("Data request #{} inserted with state {}", dr_id, dr_state);

        // Persist Data Request Database
        let f = storage_mngr::put(&BRIDGE_DB_KEY, self);
        let fut = async move {
            let res = f.await;
            match res {
                Ok(_) => log::debug!("Bridge database successfully persisted"),
                Err(e) => log::error!("Bridge database error during persistence: {}", e),
            }
        };
        ctx.spawn(fut.into_actor(self));
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
    type Result = Result<Vec<(DrId, Bytes, Hash, i64)>, ()>;

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
                        dr_info.dr_tx_creation_timestamp.unwrap(),
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
