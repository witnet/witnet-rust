use serde::Deserialize;

use crate::wallet;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ImportSeedRequest {
    Mnemonics { mnemonics: wallet::Mnemonics },
    Seed { seed: String },
}
