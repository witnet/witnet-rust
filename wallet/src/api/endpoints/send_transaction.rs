use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTransactionRequest {
    transaction_id: String,
}

pub type SendTransactionResponse = ();
