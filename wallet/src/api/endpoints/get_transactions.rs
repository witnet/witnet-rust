use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GetTransactionsRequest {
    pub wallet_id: String,
    pub limit: u32,
    pub page: u32,
}
