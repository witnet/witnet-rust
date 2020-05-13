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
    let bootstrap_hash = Hash::SHA256([3; 32]);
    let genesis_hash = Hash::SHA256([4; 32]);
    let chain_info = ChainInfo {
        environment: Environment::Mainnet,
        consensus_constants: ConsensusConstants {
            checkpoint_zero_timestamp: 0,
            checkpoints_period: 0,
            bootstrap_hash,
            genesis_hash,
            max_block_weight: 0,
            activity_period: 0,
            reputation_expire_alpha_diff: 0,
            reputation_issuance: 0,
            reputation_issuance_stop: 0,
            reputation_penalization_factor: 0.0,
            mining_backup_factor: 0,
            mining_replication_factor: 0,
            collateral_minimum: 0,
            collateral_age: 0,
            superblock_period: 0,
            extra_rounds: 0,
        },
        highest_block_checkpoint: CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: bootstrap_hash,
        },
        highest_vrf_output: CheckpointVRF {
            checkpoint: 0,
            hash_prev_vrf: bootstrap_hash,
        },
    };
    let c = ChainState {
        chain_info: Some(chain_info),
        ..ChainState::default()
    };
    t(c);
}
