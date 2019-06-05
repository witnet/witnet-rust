use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateWalletRequest {
    pub name: String,
    pub password: String,
}
