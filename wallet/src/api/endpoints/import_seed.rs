use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ImportSeedRequest {
    Mnemonics { mnemonics: String },
    Seed { seed: String },
}
