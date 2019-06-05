use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UnlockWalletRequest {
    pub id: String,
    pub password: String,
}
