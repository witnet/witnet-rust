//! # RadManager actor
//!
//! This module contains the `RadManager` actor which is in charge of
//! receiving and executing [Data Requests] using the [RAD Engine].
//!
//! [Data Requests]: https://docs.witnet.io/protocol/data-requests/overview/
//! [RAD Engine]: https://docs.witnet.io/protocol/data-requests/overview/#the-rad-engine

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
    /// Derive HTTP transports from registered proxies.
    ///
    /// This internally injects a `None` at the beginning, standing for the base "clearnet"
    /// transport (no proxy).
    pub fn get_http_transports(&self) -> Vec<Option<String>> {
        iter::once(None)
            .chain(self.proxies.iter().cloned().map(Some))
            .collect_vec()
    }
}
