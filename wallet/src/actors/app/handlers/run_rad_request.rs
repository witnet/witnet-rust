//! # Run RAD Request handler
//!
//! This handler is in charge of receiving a RAD-request (previously Data-request) and executing it.
//!
//! For more information about what a RAD-request is see [Witnet docs](https://docs.witnet.io/)

use actix::prelude::*;
use futures::Future as _;
use serde::Deserialize;

use crate::actors::{rad_executor as executor, App};
use crate::error;
use witnet_data_structures::chain::RADRequest;
use witnet_rad as rad;

/// Message containing the definition of the RAD-request to run.
#[derive(Debug, Deserialize)]
pub struct RunRadRequest(pub RADRequest);

impl RunRadRequest {}

impl Message for RunRadRequest {
    type Result = Result<rad::types::RadonTypes, error::Error>;
}

impl Handler<RunRadRequest> for App {
    type Result = ResponseFuture<rad::types::RadonTypes, error::Error>;

    fn handle(
        &mut self,
        RunRadRequest(request): RunRadRequest,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let fut = self
            .rad_executor
            .send(executor::Run(request))
            .map_err(error::Error::Mailbox)
            .and_then(|result| result.map_err(error::Error::Rad));

        Box::new(fut)
    }
}
