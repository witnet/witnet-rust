use actix::prelude::*;
use serde::{Deserialize, Serialize, Serializer};

use crate::actors::app;
use crate::types;

#[derive(Debug, Serialize, Deserialize)]
pub struct RunRadReqRequest {
    #[serde(rename = "radRequest")]
    pub rad_request: types::RADRequest,
}

#[derive(Debug, Serialize)]
pub struct RunRadReqResponse {
    #[serde(serialize_with = "debug_serialize")]
    pub result: types::RadonTypes,
}

// Serialize a type as a string, using its debug representation
fn debug_serialize<S, T>(x: &T, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: std::fmt::Debug,
{
    s.serialize_str(&format!("{:?}", x))
}

impl Message for RunRadReqRequest {
    type Result = app::Result<RunRadReqResponse>;
}

impl Handler<RunRadReqRequest> for app::App {
    type Result = app::ResponseFuture<RunRadReqResponse>;

    fn handle(&mut self, msg: RunRadReqRequest, _ctx: &mut Self::Context) -> Self::Result {
        let f = self
            .run_rad_request(msg.rad_request)
            .map_err(app::internal_error)
            .map(|result| RunRadReqResponse { result });

        Box::new(f)
    }
}
