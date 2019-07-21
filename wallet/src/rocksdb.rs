use crate::model;

/// Rocksdb merge operator for the wallet database.
pub fn merge_operator(
    new_key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &mut rocksdb::MergeOperands,
) -> Option<Vec<u8>> {
    match new_key {
        b"wallets" => {
            log::trace!("merge starting...");
            let mut infos: Vec<model::WalletInfo> = Vec::with_capacity(operands.size_hint().0);

            if let Some(bytes) = existing_val {
                infos = bincode::deserialize(bytes).expect("merge: deserialize ids failed");
            }

            for bytes in operands {
                try_merge_vec(&mut infos, bytes)
                    .or_else(|_| try_merge(&mut infos, bytes))
                    .expect("merge: deserialize operand failed");
            }
            log::trace!("merge finished");
            Some(
                bincode::serialize::<Vec<model::WalletInfo>>(infos.as_ref())
                    .expect("merge: serialize ids failed"),
            )
        }
        field => panic!("field {:?} do not support merge", field),
    }
}

fn try_merge<T>(values: &mut Vec<T>, slice: &[u8]) -> bincode::Result<()>
where
    T: serde::de::DeserializeOwned + PartialEq<T>,
{
    log::trace!("merging value");
    let val = bincode::deserialize(slice)?;

    if !values.contains(&val) {
        values.push(val);
    }

    Ok(())
}

fn try_merge_vec<T>(values: &mut Vec<T>, slice: &[u8]) -> bincode::Result<()>
where
    T: serde::de::DeserializeOwned + PartialEq<T>,
{
    log::trace!("merging vec of values");
    let old_values: Vec<T> = bincode::deserialize(slice)?;

    for val in old_values {
        if !values.contains(&val) {
            values.push(val);
        }
    }

    Ok(())
}
