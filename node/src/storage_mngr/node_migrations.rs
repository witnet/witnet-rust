use witnet_data_structures::{
    chain::{ChainState, tapi::TapiEngine},
    proto::versioning::ProtocolInfo,
    staking::stakes::StakesTracker,
    utxo_pool::UtxoWriteBatch,
};

use super::*;

macro_rules! as_failure {
    ($e:expr) => {
        anyhow::Error::from_boxed(Box::new($e))
    };
}

/// Return the version of the `ChainState` serialization. Returns error on end of file.
fn check_chain_state_version(chain_state_bytes: &[u8]) -> Result<u32, ()> {
    if chain_state_bytes.is_empty() {
        return Err(());
    }

    // Before versioning support, the first byte of the serialization of ChainState was the tag of
    // an Option, which is one byte that must be either 0 or 1.
    if chain_state_bytes[0] == 0 || chain_state_bytes[0] == 1 {
        Ok(0)
    } else {
        // After versioning support, there is a db_version before the serialization.
        // This field is a u32 (little endian) so it takes the first 4 bytes.
        // db_version % 256 must never be 0 or 1, because that can be confused with version 0.
        if chain_state_bytes.len() < 4 {
            return Err(());
        }
        let mut four_bytes = [0; 4];
        four_bytes.copy_from_slice(&chain_state_bytes[0..4]);
        let db_version = u32::from_le_bytes(four_bytes);

        Ok(db_version)
    }
}

// TODO: change signature to &mut Vec<u8> and edit the bytes in place?
// The input is assumed to be the serialization of a v0 ChainState
fn migrate_chain_state_v0_to_v2(old_chain_state_bytes: &[u8]) -> Vec<u8> {
    let db_version: u32 = 2;
    let db_version_bytes = db_version.to_le_bytes();

    // Extra fields in ChainState v2:
    let tapi = TapiEngine::default();
    let tapi_bytes = bincode::serialize(&tapi).unwrap();

    [&db_version_bytes, old_chain_state_bytes, &tapi_bytes].concat()
}

// This only needs to update the db_version field
fn migrate_chain_state_v2_to_v3(chain_state_bytes: &mut [u8]) {
    let db_version: u32 = 3;
    let db_version_bytes = db_version.to_le_bytes();
    chain_state_bytes[0..4].copy_from_slice(&db_version_bytes);
}

fn migrate_chain_state_v3_to_v4(old_chain_state_bytes: &[u8]) -> Vec<u8> {
    let db_version: u32 = 4;
    let db_version_bytes = db_version.to_le_bytes();

    // Extra fields in ChainState v4:
    let protocol_info = ProtocolInfo::default();
    let protocol_info_bytes = serialize(&protocol_info).unwrap();
    let stakes = StakesTracker::default();
    let stakes_bytes = serialize(&stakes).unwrap();

    [
        &db_version_bytes,
        &old_chain_state_bytes[4..5],
        &protocol_info_bytes,
        &old_chain_state_bytes[5..],
        &stakes_bytes,
    ]
    .concat()
}

fn migrate_chain_state_v4_to_v5(old_chain_state_bytes: &[u8]) -> Vec<u8> {
    let db_version: u32 = 5;
    let db_version_bytes = db_version.to_le_bytes();

    // Removal of fields in ChainState v5:
    let protocol_info = ProtocolInfo::default();
    let protocol_info_bytes = serialize(&protocol_info).unwrap();

    [
        &db_version_bytes,
        &old_chain_state_bytes[4..5],
        &old_chain_state_bytes[5 + protocol_info_bytes.len()..],
    ]
    .concat()
}

fn migrate_chain_state(mut bytes: Vec<u8>) -> Result<ChainState, anyhow::Error> {
    loop {
        let version = check_chain_state_version(&bytes);
        log::info!("Chain state version as read from storage is {version:?}");

        match version {
            Ok(0) => {
                // Migrate from v0 to v2
                bytes = migrate_chain_state_v0_to_v2(&bytes);
                log::info!("Successfully migrated ChainState v0 to v2");
            }
            Ok(2) => {
                // Migrate from v2 to v3
                // Actually v2 and v3 have the same serialization, the difference is that in v2 the
                // UTXOs are stored inside the ChainState, while in v3 that data structure is empty
                // and the UTXOs are stored in separate keys. But that operation is done in the
                // ChainManager on initialization, here we just update the db_version field.
                migrate_chain_state_v2_to_v3(&mut bytes);
                log::info!("Successfully migrated ChainState v2 to v3");
            }
            Ok(3) => {
                // Migrate from v3 to v4
                bytes = migrate_chain_state_v3_to_v4(&bytes);
                log::info!("Successfully migrated ChainState v3 to v4");
            }
            Ok(4) => {
                // Migrate from v3 to v4
                bytes = migrate_chain_state_v4_to_v5(&bytes);
                log::info!("Successfully migrated ChainState v4 to v5");
            }
            Ok(5) => {
                // Latest version
                // Skip the first 4 bytes because they are used to encode db_version
                return match deserialize(&bytes[4..]) {
                    Ok(v) => Ok(v),
                    Err(e) => Err(as_failure!(e)),
                };
            }
            Ok(unknown_version) => {
                return Err(anyhow::format_err!(
                    "Error when reading ChainState from database: version {} not supported",
                    unknown_version
                ));
            }
            Err(()) => {
                // Error reading version (end of file?)
                return Err(anyhow::format_err!(
                    "Error when reading ChainState version from database: unexpected end of file"
                ));
            }
        }
    }
}

/// Get value associated to key, with migrations support
fn get_versioned<K, V, F>(
    key: &K,
    migration_fn: F,
) -> impl Future<Output = Result<Option<V>, anyhow::Error>> + use<K, V, F>
where
    K: serde::Serialize,
    F: FnOnce(Vec<u8>) -> Result<V, anyhow::Error>,
{
    let addr = StorageManagerAdapter::from_registry();

    let key_bytes = match serialize(key) {
        Ok(x) => x,
        Err(e) => return futures::future::Either::Left(future::ready(Err(e.into()))),
    };

    let fut = async move {
        let opt = addr.send(Get(key_bytes)).await??;

        match opt {
            Some(bytes) => migration_fn(bytes).map(Some),
            None => Ok(None),
        }
    };

    futures::future::Either::Right(fut)
}

/// Get value associated to key
pub fn get_chain_state<K>(key: K) -> impl Future<Output = Result<Option<ChainState>, anyhow::Error>>
where
    K: serde::Serialize,
{
    get_versioned(&key, migrate_chain_state)
}

/// Put a value associated to the key into the storage, preceded by a 4-byte version tag
fn put_versioned_to_batch<K>(
    key: &K,
    value: &ChainState,
    db_version: u32,
    batch: &mut UtxoWriteBatch,
) -> Result<(), anyhow::Error>
where
    K: serde::Serialize,
{
    let key_bytes = match serialize(key) {
        Ok(x) => x,
        Err(e) => {
            return Err(e.into());
        }
    };

    let mut buf = db_version.to_le_bytes().to_vec();
    let value_bytes = match bincode::serialize_into(&mut buf, value) {
        Ok(()) => buf,
        Err(e) => {
            return Err(e.into());
        }
    };

    batch.put_raw(key_bytes, value_bytes);

    Ok(())
}

/// Put a value associated to the key into the storage.
/// The value will be atomically written along with the contents of the batch: either it will all
/// succeed or it will all fail.
// TODO: how to ensure that we don't accidentally persist the chain state using put instead of put_chain_state?
pub fn put_chain_state_in_batch<K>(
    key: &K,
    chain_state: &ChainState,
    mut batch: UtxoWriteBatch,
) -> impl Future<Output = Result<(), anyhow::Error>> + 'static
where
    K: serde::Serialize + 'static,
{
    let db_version: u32 = 5;
    // The first byte of the ChainState db_version must never be 0 or 1,
    // because that can be confused with version 0.
    assert!(db_version.to_le_bytes()[0] >= 2);

    let res = put_versioned_to_batch(key, chain_state, db_version, &mut batch);

    let addr = StorageManagerAdapter::from_registry();

    async move {
        res?;
        addr.send(Batch(batch)).await?
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use witnet_data_structures::chain::ChainInfo;

    use super::*;

    #[test]
    fn bincode_version() {
        #[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
        struct TestString0 {
            data: String,
        }

        #[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
        struct TestString1 {
            version: u32,
            data: String,
            extra: String,
        }

        let t0 = TestString0 {
            data: "data".to_string(),
        };

        let t0_bytes = bincode::serialize(&t0).unwrap();

        let t1 = TestString1 {
            version: 1,
            data: "data".to_string(),
            extra: "extra".to_string(),
        };

        let t1_bytes = bincode::serialize(&t1).unwrap();

        let version: u32 = 1;
        let version_bytes = bincode::serialize(&version).unwrap();
        let field = "extra".to_string();
        let field_bytes = bincode::serialize(&field).unwrap();
        let t0_bytes_migrated = [&version_bytes[..], &t0_bytes[..], &field_bytes[..]].concat();
        assert_eq!(t0_bytes_migrated, t1_bytes);

        let migrated_t1: TestString1 = bincode::deserialize(&t0_bytes_migrated).unwrap();
        assert_eq!(migrated_t1, t1);
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct OldChainState {
        /// Blockchain information data structure
        pub chain_info: Option<ChainInfo>,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct NewChainState {
        /// Blockchain information data structure
        pub chain_info: Option<ChainInfo>,
        /// TAPI
        pub tapi: TapiEngine,
    }

    #[test]
    fn bincode_chainstate_option() {
        // The first field of the old ChainState is an Option<_>, so the first byte of the
        // serialization will be either 0 or 1.

        let t0 = OldChainState { chain_info: None };
        let t0_bytes = bincode::serialize(&t0).unwrap();
        // An option set to None is serialized as one byte: 0
        assert_eq!(t0_bytes, vec![0]);
        // This is detected as version 0
        assert_eq!(check_chain_state_version(&t0_bytes), Ok(0));

        let default_chain_info = ChainInfo::default();

        let t1 = OldChainState {
            chain_info: Some(default_chain_info.clone()),
        };
        let t1_bytes = bincode::serialize(&t1).unwrap();
        // An option set to Some is serialized as one byte: 1, followed by the serialization of
        // the field
        assert_eq!(t1_bytes[0], 1);

        let chain_info_bytes = bincode::serialize(&default_chain_info).unwrap();
        assert_eq!(t1_bytes[1..], chain_info_bytes);

        // This is also detected as version 0
        assert_eq!(check_chain_state_version(&t1_bytes), Ok(0));
    }

    #[test]
    fn bincode_chainstate_migration() {
        let t0 = OldChainState { chain_info: None };
        let t0_bytes = bincode::serialize(&t0).unwrap();

        let t0_migrated_bytes = migrate_chain_state_v0_to_v2(&t0_bytes);
        let t0_migrated: NewChainState = bincode::deserialize(&t0_migrated_bytes[4..]).unwrap();
        assert_eq!(
            t0_migrated,
            NewChainState {
                chain_info: None,
                tapi: TapiEngine::default(),
            }
        );

        let default_chain_info = ChainInfo::default();
        let t1 = OldChainState {
            chain_info: Some(default_chain_info.clone()),
        };
        let t1_bytes = bincode::serialize(&t1).unwrap();
        let t1_migrated_bytes = migrate_chain_state_v0_to_v2(&t1_bytes);
        let t1_migrated: NewChainState = bincode::deserialize(&t1_migrated_bytes[4..]).unwrap();
        assert_eq!(
            t1_migrated,
            NewChainState {
                chain_info: Some(default_chain_info),
                tapi: TapiEngine::default(),
            }
        );
    }

    #[test]
    fn bincode_chainstate_migration_multiple_steps() {
        // This test ensures that there are no accidental infinite loops in migrate_chain_state

        // An empty ChainState v0 is 241 bytes
        let chain_state_v0_bytes = vec![0; 241];
        let migrated_chain_state = migrate_chain_state(chain_state_v0_bytes);
        migrated_chain_state.unwrap();
    }

    #[test]
    fn bincode_serialize_into() {
        let mut v = vec![0, 1, 2, 3];
        bincode::serialize_into(&mut v, &4_u8).unwrap();
        assert_eq!(v, vec![0, 1, 2, 3, 4])
    }
}
