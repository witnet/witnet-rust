use serde::{Deserialize, Serialize};

use witnet_crypto as crypto;

#[derive(Debug, Deserialize)]
pub struct CreateMnemonicsRequest {
    pub length: crypto::mnemonic::Length,
}

#[derive(Debug, Serialize)]
pub struct CreateMnemonicsResponse {
    pub mnemonics: String,
}
