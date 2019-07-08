use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct CreateVttRequest {
    address: String,
    label: String,
    amount: u64,
    fee: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVttResponse {
    pub transaction_id: String,
}
