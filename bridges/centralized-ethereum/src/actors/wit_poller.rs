use crate::{
    actors::{
        dr_database::{DrDatabase, GetAllPendingDrs},
        dr_reporter::{DrReporter, DrReporterMsg},
    },
    config::Config,
};
use actix::prelude::*;
use async_jsonrpc_client::{
    transports::{shared::EventLoopHandle, tcp::TcpSocket},
    Transport,
};
use futures_util::compat::Compat01As03;
use serde_json::json;
use std::{sync::Arc, time::Duration};
use witnet_data_structures::chain::DataRequestInfo;

/// WitPoller actor checks periodically the state of the requests in Witnet to call DrReporter
/// in case of found a tally
#[derive(Default)]
pub struct WitPoller {
    witnet_client: Option<Arc<TcpSocket>>,
    _handle: Option<EventLoopHandle>,
    wit_tally_polling_rate_ms: u64,
}

/// Make actor from WitPoller
impl Actor for WitPoller {
    /// Every actor has to provide execution Context in which it can run.
    type Context = Context<Self>;

    /// Method to be executed when the actor is started
    fn started(&mut self, ctx: &mut Self::Context) {
        log::debug!("WitPoller actor has been started!");

        self.check_tally_pending_drs(ctx, Duration::from_millis(self.wit_tally_polling_rate_ms))
    }
}

/// Required trait for being able to retrieve WitPoller address from system registry
impl actix::Supervised for WitPoller {}

/// Required trait for being able to retrieve WitPoller address from system registry
impl SystemService for WitPoller {}

impl WitPoller {
    /// Initialize `PeersManager` taking the configuration from a `Config` structure
    pub fn from_config(config: &Config) -> Result<Self, String> {
        let wit_tally_polling_rate_ms = config.wit_tally_polling_rate_ms;
        let witnet_addr = config.witnet_jsonrpc_addr.to_string();

        let (_handle, witnet_client) = TcpSocket::new(&witnet_addr).unwrap();
        let witnet_client = Arc::new(witnet_client);

        Ok(Self {
            witnet_client: Some(witnet_client),
            _handle: Some(_handle),
            wit_tally_polling_rate_ms,
        })
    }

    fn check_tally_pending_drs(&self, ctx: &mut Context<Self>, period: Duration) {
        let witnet_client = self.witnet_client.clone().unwrap();

        let fut = async move {
            let dr_database_addr = DrDatabase::from_registry();
            let dr_reporter_addr = DrReporter::from_registry();
            let pending_drs = dr_database_addr
                .send(GetAllPendingDrs)
                .await
                .unwrap()
                .unwrap();

            for (dr_id, dr_bytes, dr_tx_hash) in pending_drs {
                let report = witnet_client.execute("dataRequestReport", json!([dr_tx_hash]));

                let report = Compat01As03::new(report).await;
                let report = match report {
                    Ok(report) => report,

                    Err(e) => {
                        log::debug!(
                            "[{}] dataRequestReport call error: {}",
                            dr_id,
                            e.to_string()
                        );
                        continue;
                    }
                };

                match serde_json::from_value::<Option<DataRequestInfo>>(report) {
                    Ok(Some(DataRequestInfo {
                        tally: Some(tally), ..
                    })) => {
                        log::info!(
                            "[{}] Found possible tally to be reported for dr_tx_hash {}",
                            dr_id,
                            dr_tx_hash
                        );

                        let result = tally.tally;
                        dr_reporter_addr
                            .send(DrReporterMsg {
                                dr_id,
                                dr_bytes,
                                dr_tx_hash,
                                result,
                            })
                            .await
                            .unwrap();
                    }
                    Ok(..) => {
                        // No problem, this means the data request has not been resolved yet
                        log::debug!("[{}] Data request not resolved yet", dr_id);
                        continue;
                    }
                    Err(e) => {
                        log::error!("[{}] dataRequestReport deserialize error: {:?}", dr_id, e);
                        continue;
                    }
                };
            }
        };

        ctx.spawn(fut.into_actor(self).then(move |(), _act, ctx| {
            // Wait until the function finished to schedule next call.
            // This avoids tasks running in parallel.
            ctx.run_later(period, move |act, ctx| {
                // Reschedule check_tally_pending_drs
                act.check_tally_pending_drs(ctx, period);
            });

            actix::fut::ready(())
        }));
    }
}
