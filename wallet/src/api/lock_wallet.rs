use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct LockWalletRequest {
    pub wallet_id: String,
    #[serde(default)]
    pub wipe: bool,
}
