use crate::chain::{OutputPointer, PublicKeyHash, ValueTransferOutput};
use failure::Error;
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
    sync::RwLock,
};
use witnet_storage::storage::{Storage, StorageIterator, WriteBatch, WriteBatchItem};

/// Database that stores UTXOs
pub trait UtxoDb {
    fn get_utxo(
        &self,
        out_ptr: &OutputPointer,
    ) -> Result<Option<(ValueTransferOutput, u32)>, failure::Error>;
    fn utxo_iterator(&self) -> Result<UtxoStorageIterator, failure::Error>;
    fn utxo_iterator_by_pkh(
        &self,
        pkh: PublicKeyHash,
    ) -> Result<UtxoStorageIterator, failure::Error>;
    fn write(&self, batch: UtxoWriteBatch) -> Result<(), failure::Error>;
}

#[derive(Default)]
pub struct UtxoWriteBatch {
    v: Vec<UtxoWriteBatchItem>,
}

enum UtxoWriteBatchItem {
    Put(OutputPointer, (ValueTransferOutput, u32)),
    Delete(OutputPointer),
    Raw(WriteBatchItem),
}

impl UtxoWriteBatch {
    pub fn put(&mut self, k: OutputPointer, v: (ValueTransferOutput, u32)) {
        self.v.push(UtxoWriteBatchItem::Put(k, v));
    }
    pub fn put_raw(&mut self, k: Vec<u8>, v: Vec<u8>) {
        self.v
            .push(UtxoWriteBatchItem::Raw(WriteBatchItem::Put(k, v)));
    }
    pub fn delete(&mut self, k: OutputPointer) {
        self.v.push(UtxoWriteBatchItem::Delete(k));
    }
}

impl From<UtxoWriteBatch> for WriteBatch {
    fn from(x: UtxoWriteBatch) -> Self {
        let mut batch = WriteBatch::default();

        for item in x.v {
            match item {
                UtxoWriteBatchItem::Put(k, v) => {
                    let key_string = format!("UTXO-{}", k);
                    batch.put(
                        key_string.into_bytes(),
                        bincode::serialize(&v).expect("bincode fail"),
                    );
                }
                UtxoWriteBatchItem::Delete(k) => {
                    let key_string = format!("UTXO-{}", k);
                    batch.delete(key_string.as_bytes().to_vec());
                }
                UtxoWriteBatchItem::Raw(x) => {
                    batch.batch.push(x);
                }
            }
        }

        batch
    }
}

/// Iterator over key-value pairs
pub type UtxoStorageIterator<'a> =
    Box<dyn Iterator<Item = (OutputPointer, (ValueTransferOutput, u32))> + 'a>;

/// Wrap a `Storage` implementation that allows to put and delete raw bytes in a way that allows
/// storing `OutputPointer`s. UTXOs are prefixed as `"UTXO-"`, followed by the string representation
/// of the `OutputPointer`. The value stored is a tuple of `(ValueTransferOutput, u32)` serialized
/// using bincode.
///
/// For example: `"UTXO-0222222222222222222222222222222222222222222222222222222222222222:1"`
#[derive(Debug)]
pub struct UtxoDbWrapStorage<S>(pub S);

// The Storage implementation simply forwards to the inner Storage.
impl<S: Storage> Storage for UtxoDbWrapStorage<S> {
    fn get(&self, key: &[u8]) -> witnet_storage::storage::Result<Option<Vec<u8>>> {
        self.0.get(key)
    }

    fn put(&self, key: Vec<u8>, value: Vec<u8>) -> witnet_storage::storage::Result<()> {
        self.0.put(key, value)
    }

    fn delete(&self, key: &[u8]) -> witnet_storage::storage::Result<()> {
        self.0.delete(key)
    }

    fn prefix_iterator<'a, 'b: 'a>(
        &'a self,
        prefix: &'b [u8],
    ) -> witnet_storage::storage::Result<StorageIterator<'a>> {
        self.0.prefix_iterator(prefix)
    }

    fn write(&self, batch: WriteBatch) -> Result<(), failure::Error> {
        self.0.write(batch)
    }
}

// The UtxoDb implementation handles the conversion from UTXOs and ValueTransferOutputs into raw
// bytes.
impl<S: Storage> UtxoDb for UtxoDbWrapStorage<S> {
    fn get_utxo(&self, k: &OutputPointer) -> Result<Option<(ValueTransferOutput, u32)>, Error> {
        let key_string = format!("UTXO-{}", k);

        Ok(self.0.get(key_string.as_bytes())?.map(|bytes| {
            bincode::deserialize::<(ValueTransferOutput, u32)>(&bytes).expect("bincode fail")
        }))
    }

    fn utxo_iterator(&self) -> Result<UtxoStorageIterator, Error> {
        let iter = self.0.prefix_iterator(b"UTXO-")?;

        Ok(Box::new(iter.map(|(k, v)| {
            let key_string = String::from_utf8(k).unwrap();
            let output_pointer_str = key_string.strip_prefix("UTXO-").unwrap();
            let key = OutputPointer::from_str(output_pointer_str).unwrap();
            let value = bincode::deserialize(&v).unwrap();

            (key, value)
        })))
    }

    fn utxo_iterator_by_pkh(&self, pkh: PublicKeyHash) -> Result<UtxoStorageIterator, Error> {
        let iter = self.utxo_iterator()?;

        Ok(Box::new(iter.filter_map(move |(k, v)| {
            if v.0.pkh == pkh {
                Some((k, v))
            } else {
                None
            }
        })))
    }

    fn write(&self, batch: UtxoWriteBatch) -> Result<(), Error> {
        self.0.write(batch.into())
    }
}

/// Wrap a `UtxoDb` implementation and add a cache of `address` to `list of UTXOs`.
#[derive(Debug)]
pub struct CacheUtxosByPkh<S> {
    db: S,
    // Need to use a RwLock<HashMap<K, HashSet<V>> as the cache because:
    // * write needs to be able to update the hashmap using an immutable reference.
    // * write needs to be able to quickly insert and remove elements, so `HashSet<V>` is better than `Vec<V>`.
    // * utxo_iterator_by_pkh needs to be able to borrow a value from the hashmap, so `RwLock` is better than `Mutex`.
    cache: RwLock<HashMap<PublicKeyHash, HashSet<OutputPointer>>>,
}

impl<S: UtxoDb> CacheUtxosByPkh<S> {
    pub fn new(db: S) -> Result<Self, failure::Error> {
        Self::new_with_progress(db, |_| {})
    }

    pub fn new_with_progress<F: FnMut(usize)>(
        db: S,
        mut progress_cb: F,
    ) -> Result<Self, failure::Error> {
        let mut cache: HashMap<PublicKeyHash, HashSet<OutputPointer>> = HashMap::default();

        // Initialize cache
        for (i, (k, v)) in db.utxo_iterator()?.enumerate() {
            cache.entry(v.0.pkh).or_default().insert(k);
            // Allow caller to log progress because this iterator may take a few seconds on mainnet.
            progress_cb(i);
        }

        Ok(Self {
            db,
            cache: RwLock::new(cache),
        })
    }
}

// The Storage implementation simply forwards to the inner Storage.
impl<S: Storage> Storage for CacheUtxosByPkh<S> {
    fn get(&self, key: &[u8]) -> witnet_storage::storage::Result<Option<Vec<u8>>> {
        self.db.get(key)
    }

    fn put(&self, key: Vec<u8>, value: Vec<u8>) -> witnet_storage::storage::Result<()> {
        self.db.put(key, value)
    }

    fn delete(&self, key: &[u8]) -> witnet_storage::storage::Result<()> {
        self.db.delete(key)
    }

    fn prefix_iterator<'a, 'b: 'a>(
        &'a self,
        prefix: &'b [u8],
    ) -> witnet_storage::storage::Result<StorageIterator<'a>> {
        self.db.prefix_iterator(prefix)
    }

    fn write(&self, batch: WriteBatch) -> witnet_storage::storage::Result<()> {
        self.db.write(batch)
    }
}

// The UtxoDb implementation forwards to the inner UtxoDb, except for the utxo_iterator_by_pkh
// method, which uses the cache, and the write method which updates the cache before forwarding the
// call.
impl<S: UtxoDb> UtxoDb for CacheUtxosByPkh<S> {
    fn get_utxo(&self, k: &OutputPointer) -> Result<Option<(ValueTransferOutput, u32)>, Error> {
        self.db.get_utxo(k)
    }

    fn utxo_iterator(&self) -> Result<UtxoStorageIterator, Error> {
        self.db.utxo_iterator()
    }

    fn utxo_iterator_by_pkh(&self, pkh: PublicKeyHash) -> Result<UtxoStorageIterator, Error> {
        let cache = self.cache.read().unwrap();
        // We must clone the list of UTXOs to be able to return an iterator over it, because
        // otherwise the iterator would need to get ownership of the RwLockReadGuard somehow.
        let utxos_of_pkh: Vec<OutputPointer> = cache
            .get(&pkh)
            .map(|hs| hs.iter().cloned().collect())
            .unwrap_or_default();

        let iter = utxos_of_pkh.into_iter().map(move |out_ptr| {
            // Move ownership of cache RwLockReadGuard into this closure to ensure that it is not
            // possible to acquire a write lock over the cache while this iterator is alive.
            let _cache = &cache;
            // TODO: we could return an error instead of the first unwrap here, but that would force
            // UtxoStorageIterator to return a Result
            let vto = self.db.get_utxo(&out_ptr).unwrap().unwrap();

            (out_ptr, vto)
        });

        Ok(Box::new(iter))
    }

    fn write(&self, batch: UtxoWriteBatch) -> Result<(), Error> {
        // Update self.cache taking into account batch
        let mut cache = self.cache.write().unwrap();

        for item in &batch.v {
            match item {
                UtxoWriteBatchItem::Put(k, v) => {
                    cache.entry(v.0.pkh).or_default().insert(*k);
                }
                UtxoWriteBatchItem::Delete(k) => {
                    let vto = self.db.get_utxo(k)?.unwrap();
                    let hs = cache.get_mut(&vto.0.pkh).unwrap();
                    hs.remove(k);
                    // Remove entry from cache if hashset is empty
                    if hs.is_empty() {
                        cache.remove(&vto.0.pkh);
                    }
                }
                UtxoWriteBatchItem::Raw(_) => {}
            }
        }

        self.db.write(batch)
    }
}
