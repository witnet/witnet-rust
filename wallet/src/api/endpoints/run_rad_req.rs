use serde::{Deserialize, Serialize};
use witnet_rad as rad;

use crate::wallet;

#[derive(Debug, Deserialize)]
pub struct RunRadReqRequest {
    #[serde(rename = "radRequest")]
    pub rad_request: wallet::RADRequest,
}

#[derive(Debug, Serialize)]
pub struct RunRadReqResponse {
    pub result: rad::types::RadonTypes,
}
