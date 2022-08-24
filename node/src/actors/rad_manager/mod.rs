//! # RadManager actor
//!
//! This module contains the `RadManager` actor which is in charge of
//! receiving and executing [Data Requests] using the [RAD Engine].
//!
//! [Data Requests]: https://docs.witnet.io/protocol/data-requests/overview/
//! [RAD Engine]: https://docs.witnet.io/protocol/data-requests/overview/#the-rad-engine

use crate::utils::stop_system_if_panicking;
use witnet_data_structures::witnessing::WitnessingConfig;

mod actor;
mod handlers;

/// RadManager actor
#[derive(Debug, Default)]
pub struct RadManager {
    /// Contains configuration for witnessing, namely about transports and the paranoid threshold.
    pub witnessing: WitnessingConfig<witnet_rad::Uri>,
}

impl RadManager {
    /// Register a new proxy address to be used for "paranoid retrieval", i.e. retrieving data
    /// sources through different transports so as to ensure that the data sources are consistent
    /// and we are taking as small of a risk as possible when committing to specially crafted data
    /// requests that may be potentially ill-intended.
    pub fn add_proxy(&mut self, proxy_address: witnet_rad::Uri) {
        self.witnessing.transports.push(Some(proxy_address))
    }

    /// Construct a `RadManager` from existing witnessing configuration.
    pub fn from_config(config: WitnessingConfig<witnet_rad::Uri>) -> Self {
        Self { witnessing: config }
    }
}

impl Drop for RadManager {
    fn drop(&mut self) {
        log::trace!("Dropping RadManager");
        // RadManager handles radon panics so it should never stop because of a panic.
        // That's handled by ensuring that the panics always happen inside a future.
        // If for some reason RadManager panics outside of a future, then we want to stop the actor
        // system.
        stop_system_if_panicking("RadManager");
    }
}
