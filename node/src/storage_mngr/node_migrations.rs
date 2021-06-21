use super::*;
use witnet_data_structures::{chain::ChainState, mainnet_validations::TapiEngine};

macro_rules! as_failure {
    ($e:expr) => {
        failure::Error::from_boxed_compat(Box::new($e))
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
    let db_version_bytes = db_version.to_be_bytes();

    // Extra fields in ChainState v2:
    let tapi = TapiEngine::default();
    let tapi_bytes = bincode::serialize(&tapi).unwrap();

    [&db_version_bytes, old_chain_state_bytes, &tapi_bytes].concat()
}

fn migrate_chain_state(bytes: &[u8]) -> Result<ChainState, failure::Error> {
    match check_chain_state_version(&bytes) {
        Ok(0) => {
            // Migrate from v0 to v2
            let bytes = migrate_chain_state_v0_to_v2(&bytes);
            log::debug!("Successfully migrated ChainState v0 to v2");

            // Latest version
            // Skip the first 4 bytes because they are used to encode db_version
            match deserialize(&bytes[4..]) {
                Ok(v) => Ok(v),
                Err(e) => Err(as_failure!(e)),
            }
        }
        Ok(2) => {
            // Latest version
            // Skip the first 4 bytes because they are used to encode db_version
            match deserialize(&bytes[4..]) {
                Ok(v) => Ok(v),
                Err(e) => Err(as_failure!(e)),
            }
        }
        Ok(unknown_version) => Err(failure::format_err!(
            "Error when reading ChainState from database: version {} not supported",
            unknown_version
        )),
        Err(()) => {
            // Error reading version (end of file?)
            Err(failure::format_err!(
                "Error when reading ChainState version from database: unexpected end of file"
            ))
        }
    }
}

/// Get value associated to key, with migrations support
fn get_versioned<K, V, F>(
    key: &K,
    migration_fn: F,
) -> impl Future<Output = Result<Option<V>, failure::Error>>
where
    K: serde::Serialize,
    F: FnOnce(Vec<u8>) -> Result<V, failure::Error>,
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
pub fn get_chain_state<K>(
    key: &K,
) -> impl Future<Output = Result<Option<ChainState>, failure::Error>>
where
    K: serde::Serialize,
{
    get_versioned(key, |bytes| migrate_chain_state(&bytes))
}

/// Put a value associated to the key into the storage, preceded by a 4-byte version tag
fn put_versioned<'a, 'b, K>(
    key: &'a K,
    value: &'b ChainState,
    db_version: u32,
) -> impl Future<Output = Result<(), failure::Error>> + 'static
where
    K: serde::Serialize,
{
    let addr = StorageManagerAdapter::from_registry();

    let key_bytes = match serialize(key) {
        Ok(x) => x,
        Err(e) => {
            return futures::future::Either::Left(futures::future::Either::Right(future::ready(
                Err(e.into()),
            )))
        }
    };

    let mut buf = db_version.to_le_bytes().to_vec();
    let value_bytes = match bincode::serialize_into(&mut buf, value) {
        Ok(()) => buf,
        Err(e) => {
            return futures::future::Either::Left(futures::future::Either::Left(future::ready(
                Err(e.into()),
            )))
        }
    };

    futures::future::Either::Right(async move { addr.send(Put(key_bytes, value_bytes)).await? })
}

/// Put a value associated to the key into the storage
// TODO: how to ensure that we don't accidentally persist the chain state using put instead of put_chain_state?
pub fn put_chain_state<'a, 'b, K>(
    key: &'a K,
    chain_state: &'b ChainState,
) -> impl Future<Output = Result<(), failure::Error>> + 'static
where
    K: serde::Serialize + 'static,
{
    let db_version: u32 = 2;
    // The first byte of the ChainState db_version must never be 0 or 1,
    // because that can be confused with version 0.
    assert!(db_version.to_le_bytes()[0] >= 2);
    put_versioned(key, chain_state, db_version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use witnet_data_structures::chain::ChainInfo;

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
    fn bincode_serialize_into() {
        let mut v = vec![0, 1, 2, 3];
        bincode::serialize_into(&mut v, &4_u8).unwrap();
        assert_eq!(v, vec![0, 1, 2, 3, 4])
    }
}
