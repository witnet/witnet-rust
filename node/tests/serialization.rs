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
            max_vt_weight: 0,
            max_dr_weight: 0,
            activity_period: 0,
            reputation_expire_alpha_diff: 0,
            reputation_issuance: 0,
            reputation_issuance_stop: 0,
            reputation_penalization_factor: 0.0,
            mining_backup_factor: 0,
            mining_replication_factor: 0,
            collateral_minimum: 0,
            bootstrapping_committee: vec![],
            collateral_age: 0,
            superblock_period: 0,
            extra_rounds: 0,
            minimum_difficulty: 0,
            epochs_with_minimum_difficulty: 0,
            superblock_signing_committee_size: 100,
            superblock_committee_decreasing_period: 100,
            superblock_committee_decreasing_step: 5,
            initial_block_reward: 250 * 1_000_000_000,
            halving_period: 3_500_000,
        },
        highest_block_checkpoint: CheckpointBeacon {
            checkpoint: 0,
            hash_prev_block: bootstrap_hash,
        },
        highest_superblock_checkpoint: CheckpointBeacon {
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

#[test]
fn rad_retrieve() {
    let a = RADRetrieve {
        kind: RADType::HttpGet,
        url: "http://127.0.0.1".to_string(),
        script: vec![128],
        body: vec![],
        headers: vec![],
    };

    let bytes = serialize(&a).unwrap();

    assert_eq!(
        bytes,
        vec![
            3, 0, 0, 0, 1, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 104, 116, 116, 112, 58, 47, 47, 49,
            50, 55, 46, 48, 46, 48, 46, 49, 1, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0
        ]
    );

    t(a)
}

#[test]
fn rad_retrieve_vec() {
    let a = RADRetrieve {
        kind: RADType::HttpGet,
        url: "http://127.0.0.1".to_string(),
        script: vec![128],
        body: vec![],
        headers: vec![],
    };
    let b = a.clone();

    let x = vec![a, b];

    let bytes = serialize(&x).unwrap();

    // If we had optional fields, it would be impossible to know where does the first RADRetrieve
    // end and where does the second RADRetrieve begin
    assert_eq!(
        bytes,
        vec![
            2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 104, 116, 116,
            112, 58, 47, 47, 49, 50, 55, 46, 48, 46, 48, 46, 49, 1, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0,
            0, 104, 116, 116, 112, 58, 47, 47, 49, 50, 55, 46, 48, 46, 48, 46, 49, 1, 0, 0, 0, 0,
            0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );

    t(x)
}

#[test]
fn deserialize_rad_retrieve_old_version_unknown() {
    let retrieve_unknown: RADRetrieve = deserialize(&[
        0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 104, 116, 116, 112, 58, 47, 47, 49, 50, 55, 46, 48,
        46, 48, 46, 49, 1, 0, 0, 0, 0, 0, 0, 0, 128,
    ])
    .unwrap();

    assert_eq!(
        retrieve_unknown,
        RADRetrieve {
            kind: RADType::Unknown,
            url: "http://127.0.0.1".to_string(),
            script: vec![128],
            body: vec![],
            headers: vec![]
        }
    );

    t(retrieve_unknown);
}

#[test]
fn deserialize_rad_retrieve_old_version_http_get() {
    let retrieve_http_get: RADRetrieve = deserialize(&[
        1, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 104, 116, 116, 112, 58, 47, 47, 49, 50, 55, 46, 48,
        46, 48, 46, 49, 1, 0, 0, 0, 0, 0, 0, 0, 128,
    ])
    .unwrap();

    assert_eq!(
        retrieve_http_get,
        RADRetrieve {
            kind: RADType::HttpGet,
            url: "http://127.0.0.1".to_string(),
            script: vec![128],
            body: vec![],
            headers: vec![]
        }
    );

    t(retrieve_http_get);
}

#[test]
fn deserialize_rad_retrieve_old_version_rng() {
    let retrieve_rng: RADRetrieve = deserialize(&[
        2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 128,
    ])
    .unwrap();

    assert_eq!(
        retrieve_rng,
        RADRetrieve {
            kind: RADType::Rng,
            url: "".to_string(),
            script: vec![128],
            body: vec![],
            headers: vec![]
        }
    );

    t(retrieve_rng);
}
