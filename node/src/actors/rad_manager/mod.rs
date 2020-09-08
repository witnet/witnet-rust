//! # RadManager actor
//!
//! This module contains the `RadManager` actor which is in charge of
//! receiving and executing [Data Requests] using the [RAD Engine].
//!
//! [Data Requests]: https://docs.witnet.io/protocol/data-requests/overview/
//! [RAD Engine]: https://docs.witnet.io/protocol/data-requests/overview/#the-rad-engine

use crate::utils::stop_system_if_panicking;

use core::iter;
use itertools::Itertools;

mod actor;
mod handlers;

/// RadManager actor
#[derive(Default)]
pub struct RadManager {
    /// Addresses of proxies to be used as extra transports when performing data retrieval.
    proxies: Vec<String>,
}

impl RadManager {
    /// Register a new proxy address to be used for "paranoid retrieval", i.e. retrieving data
    /// sources through different transports so as to ensure that the data sources are consistent
    /// and we are taking as small of a risk as possible when committing to specially crafted data
    /// requests that may be potentially ill-intended.
    pub fn add_proxy(&mut self, proxy_address: String) {
        self.proxies.push(proxy_address)
    }

    /// Derive HTTP transports from registered proxies.
    ///
    /// This internally injects a `None` at the beginning, standing for the base "clearnet"
    /// transport (no proxy).
    pub fn get_http_transports(&self) -> Vec<Option<String>> {
        iter::once(None)
            .chain(self.proxies.iter().cloned().map(Some))
            .collect_vec()
    }

    /// Construct a `RadManager` with some initial proxy addresses.
    pub fn with_proxies(proxies: Vec<String>) -> Self {
        log::debug!("Configuring retrieval proxies: {:?}", proxies);

        Self { proxies }
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
