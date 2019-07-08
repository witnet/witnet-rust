use serde::Deserialize;

use crate::wallet;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDataReqRequest {
    pub rad_request: wallet::RADRequest,
}

pub type CreateDataReqResponse = ();
