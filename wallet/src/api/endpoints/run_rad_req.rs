use serde::{Deserialize, Serialize};
use witnet_rad as rad;

use crate::wallet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRadReqRequest {
    pub rad_request: wallet::RADRequest,
}

#[derive(Debug, Serialize)]
pub struct RunRadReqResponse {
    pub result: rad::types::RadonTypes,
}
