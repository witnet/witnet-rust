use serde::{Deserialize, Serialize};

mod validation;

pub use validation::*;

#[derive(Debug, Deserialize)]
pub struct CreateMnemonicsRequest {
    pub length: u8,
}

#[derive(Debug, Serialize)]
pub struct CreateMnemonicsResponse {
    pub mnemonics: String,
}
