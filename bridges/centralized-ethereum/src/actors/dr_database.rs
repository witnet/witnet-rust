use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::{cmp, collections::hash_map::Entry, collections::HashMap, fmt, future::Future};
use web3::{ethabi::Bytes, types::U256};
use witnet_data_structures::chain::Hash;
use witnet_node::{storage_mngr, utils::stop_system_if_panicking};

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

impl DrDatabase {
    // Persist Data Request Database
    fn persist(&mut self) -> impl Future<Output = ()> {
        let f = storage_mngr::put(&BRIDGE_DB_KEY, self);

        async move {
            match f.await {
                Ok(_) => log::debug!("Bridge database successfully persisted"),
                Err(e) => log::error!("Bridge database error during persistence: {}", e),
            }
        }
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
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
pub enum DrState {
    /// New: a new query was detected on the Witnet Oracle contract,
    /// but has not yet been attended.
    #[default]
    New,
    /// Pending: a data request transaction was broadcasted to the Witnet blockchain,
    /// but has not yet been resolved.
    Pending,
    /// Finished: the data request result was reported back to the Witnet Oracle contract.
    Finished,
    /// Dismissed: the data request result cannot be reported back to the Witnet Oracle contract,
    /// or was already reported by another bridge instance.
    Dismissed,
}

impl fmt::Display for DrState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DrState::New => "New",
            DrState::Pending => "Pending",
            DrState::Finished => "Finished",
            DrState::Dismissed => "Dismissed",
        };

        f.write_str(s)
    }
}

/// Possible query states in the Witnet Oracle contract
#[derive(Serialize, Deserialize, Clone)]
pub enum WitnetQueryStatus {
    /// Unknown: the query does not exist, or got eventually deleted.
    Unknown,
    /// Posted: the query exists, but has not yet been reported.
    Posted,
    /// Reported: some query result got stored into the WitnetOracle, although not yet finalized.
    Reported,
    /// Finalized: the query was reported, and considered to be final.
    Finalized,
}

impl WitnetQueryStatus {
    /// Maps uint8 to WitnetQueryStatus enum
    pub fn from_code(i: u8) -> Self {
        match i {
            1 => WitnetQueryStatus::Posted,
            2 => WitnetQueryStatus::Reported,
            3 => WitnetQueryStatus::Finalized,
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
                            log::info!("Database loaded from storage");
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

/// Set state of given data request id
pub struct SetDrState {
    /// Data Request id
    pub dr_id: DrId,
    /// Data Request new state
    pub dr_state: DrState,
}

impl Message for SetDrState {
    type Result = Result<(), ()>;
}

/// Count number of data requests in given state
pub struct CountDrsPerState;

impl Message for CountDrsPerState {
    type Result = Result<(u64, u64, u64, u64), ()>;
}

impl Handler<SetDrInfoBridge> for DrDatabase {
    type Result = ();

    fn handle(&mut self, msg: SetDrInfoBridge, ctx: &mut Self::Context) -> Self::Result {
        let SetDrInfoBridge(dr_id, dr_info) = msg;
        let dr_state = dr_info.dr_state;
        self.dr.insert(dr_id, dr_info);

        self.max_dr_id = cmp::max(self.max_dr_id, dr_id);
        log::debug!("Data request #{} inserted with state {}", dr_id, dr_state);

        // Persist Data Request Database
        ctx.spawn(self.persist().into_actor(self));
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

impl Handler<SetDrState> for DrDatabase {
    type Result = Result<(), ()>;

    fn handle(&mut self, msg: SetDrState, ctx: &mut Self::Context) -> Self::Result {
        let SetDrState { dr_id, dr_state } = msg;
        match self.dr.entry(dr_id) {
            Entry::Occupied(entry) => {
                entry.into_mut().dr_state = DrState::Finished;
                log::debug!("Data request #{} updated to state {}", dr_id, dr_state,);
            }
            Entry::Vacant(entry) => {
                entry.insert(DrInfoBridge {
                    dr_bytes: vec![],
                    dr_state,
                    dr_tx_hash: None,
                    dr_tx_creation_timestamp: None,
                });
                log::debug!("Data request #{} inserted with state {}", dr_id, dr_state,);
            }
        }

        self.max_dr_id = cmp::max(self.max_dr_id, dr_id);

        // Persist Data Request Database
        ctx.spawn(self.persist().into_actor(self));

        Ok(())
    }
}

impl Handler<CountDrsPerState> for DrDatabase {
    type Result = Result<(u64, u64, u64, u64), ()>;

    fn handle(&mut self, _msg: CountDrsPerState, _ctx: &mut Self::Context) -> Self::Result {
        let mut drs_new = u64::default();
        let mut drs_pending = u64::default();
        let mut drs_finished = u64::default();
        let mut drs_dismissed = u64::default();

        self.dr.iter().for_each(|(_dr_id, dr_info)| {
            match dr_info.dr_state {
                DrState::New => drs_new += 1,
                DrState::Pending => drs_pending += 1,
                DrState::Finished => drs_finished += 1,
                DrState::Dismissed => drs_dismissed += 1,
            };
        });

        Ok((drs_new, drs_pending, drs_finished, drs_dismissed))
    }
}

/// Required trait for being able to retrieve DrDatabase address from system registry
impl actix::Supervised for DrDatabase {}

/// Required trait for being able to retrieve DrDatabase address from system registry
impl SystemService for DrDatabase {}
