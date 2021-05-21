//! # RadManager actor
//!
//! This module contains the `RadManager` actor which is in charge of
//! receiving and executing [Data Requests] using the [RAD Engine].
//!
//! [Data Requests]: https://docs.witnet.io/protocol/data-requests/overview/
//! [RAD Engine]: https://docs.witnet.io/protocol/data-requests/overview/#the-rad-engine

mod actor;
mod handlers;

/// RadManager actor
#[derive(Default)]
pub struct RadManager;

impl Drop for RadManager {
    fn drop(&mut self) {
        log::trace!("Dropping RadManager");
        // TODO: RadManager is expected to restart on panic, ensure
        //stop_system_if_panicking("RadManager");
    }
}
