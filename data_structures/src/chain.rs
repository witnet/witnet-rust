#[derive(Debug)]
pub struct ChainInfo {
    environment: Environment,
    consensus_constants: ConsensusConstants,
    highest_block_checkpoint: Checkpoint
}

/// Possible values for the "environment" configuration param.
#[derive(Deserialize, Clone, Debug, PartialEq)]
pub enum Environment {
    /// "mainnet" environment
    #[serde(rename = "mainnet")]
    Mainnet,
    /// "testnet" environment
    #[serde(rename = "testnet-1")]
    Testnet1,
}

impl Default for Environment {
    fn default() -> Environment {
        Environment::Testnet1
    }
}

/// Consensus-critical configuration
#[derive(Debug, Clone, PartialEq)]
pub struct ConsensusConstants {
    /// Timestamp at checkpoint 0 (the start of epoch 0)
    pub checkpoint_zero_timestamp: i64,

    /// Seconds between the start of an epoch and the start of the next one
    pub checkpoints_period: u16,

    pub genesis_hash: Vec<u8>,
    pub reputation_demurrage: f64,
    pub reputation_punishment: f64,
}

#[derive(Debug)]
pub struct Checkpoint {
    number: u32,
    hash: Vec<u8>
}
