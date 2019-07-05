use serde::Deserialize;

use crate::types;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDataReqRequest {
    pub rad_request: types::RADRequest,
}

pub type CreateDataReqResponse = ();
