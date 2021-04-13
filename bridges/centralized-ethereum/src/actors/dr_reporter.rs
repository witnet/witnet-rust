use crate::actors::dr_database::DrId;
use actix::prelude::*;

/// EthPoller (TODO: Explanation)
#[derive(Default)]
pub struct DrReporter;

/// Make actor from EthPoller
impl Actor for DrReporter {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, _ctx: &mut Self::Context) {
        log::debug!("DrReporter actor has been started!");
    }
}

/// Required trait for being able to retrieve DrReporter address from system registry
impl actix::Supervised for DrReporter {}

/// Required trait for being able to retrieve DrReporter address from system registry
impl SystemService for DrReporter {}

/// Report the result of this data request id to ethereum
pub struct DrReporterMsg {
    /// Data request id in ethereum
    pub dr_id: DrId,
    /// Data request result from witnet, in bytes
    pub result: Vec<u8>,
}

impl Message for DrReporterMsg {
    type Result = ();
}

impl Handler<DrReporterMsg> for DrReporter {
    type Result = ();

    fn handle(&mut self, _msg: DrReporterMsg, _ctx: &mut Self::Context) -> Self::Result {
        // TODO: create ethereum transaction and set database state to finished
        todo!()
    }
}
