use bincode::{deserialize, serialize};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;
use witnet_data_structures::chain::*;

fn t<T>(al: T)
where
    T: Serialize + DeserializeOwned + Debug + PartialEq,
{
    let y = serialize(&al).unwrap();
    let ar = deserialize(&y).unwrap();
    assert_eq!(al, ar);
}

#[test]
fn chain_state() {
    let genesis_hash = Hash::SHA256([3; 32]);
    let chain_info = ChainInfo {
        environment: Environment::Mainnet,
        consensus_constants: ConsensusConstants {
            checkpoint_zero_timestamp: 0,
            checkpoints_period: 0,
            genesis_hash,
            reputation_demurrage: 0.0,
            reputation_punishment: 0.0,
            max_block_weight: 0,
        },
        highest_block_checkpoint: CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: genesis_hash,
        },
    };
    let c = ChainState {
        chain_info: Some(chain_info),
        ..ChainState::default()
    };
    t(c);
}
