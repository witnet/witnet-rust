use serde::{Deserialize, Serialize};

use crate::types;

#[derive(Debug, Deserialize)]
pub struct WalletInfosRequest;

#[derive(Debug, Serialize)]
pub struct WalletInfosResponse {
    pub total: usize,
    pub infos: Vec<types::WalletInfo>,
}
