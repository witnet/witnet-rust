use serde::{Deserialize, Serialize};

use crate::types;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRadReqRequest {
    pub rad_request: types::RADRequest,
}

#[derive(Debug, Serialize)]
pub struct RunRadReqResponse {
    pub result: types::RadonTypes,
}
