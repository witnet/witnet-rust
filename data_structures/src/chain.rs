#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ChainInfo {
    /// Blockchain valid environment
    pub environment: Environment,

    /// Blockchain Protocol constants
    pub consensus_constants: ConsensusConstants,

    /// Checkpoint of the last block in the blockchain
    pub highest_block_checkpoint: Checkpoint,
}

/// Possible values for the "environment" configuration param.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsensusConstants {
    /// Timestamp at checkpoint 0 (the start of epoch 0)
    pub checkpoint_zero_timestamp: i64,

    /// Seconds between the start of an epoch and the start of the next one
    pub checkpoints_period: u16,

    /// Genesis block hash value
    pub genesis_hash: Vec<u8>,

    /// Decay value for reputation demurrage function
    //TODO Use fixed point arithmetic
    pub reputation_demurrage: f64,

    /// Punishment value for dishonestly use
    //TODO Use fixed point arithmetic
    pub reputation_punishment: f64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Checkpoint {
    pub number: u32,
    pub hash: Vec<u8>,
}

impl Default for Checkpoint {
    fn default() -> Checkpoint {
        Checkpoint {
            number: 0,
            hash: Vec::new(),
        }
    }
}
