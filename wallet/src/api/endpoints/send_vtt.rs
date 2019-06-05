use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SendVttRequest {
    pub wallet_id: String,
    pub to_address: Vec<u8>,
    pub amount: u64,
    pub fee: u64,
    pub subject: String,
}
