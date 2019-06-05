use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GenerateAddressRequest {
    pub wallet_id: String,
}
