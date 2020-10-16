use std::{cell::RefCell, collections::HashMap, rc::Rc};

use super::*;

type Bytes = Vec<u8>;

#[derive(Default, Clone)]
pub struct HashMapDb {
    rc: Rc<RefCell<HashMap<Bytes, Bytes>>>,
}

impl HashMapDb {
    pub fn new(rc: Rc<RefCell<HashMap<Bytes, Bytes>>>) -> Self {
        Self { rc }
    }
}

impl std::iter::FromIterator<(Bytes, Bytes)> for HashMapDb {
    fn from_iter<I: IntoIterator<Item = (Bytes, Bytes)>>(iter: I) -> Self {
        Self::new(Rc::new(RefCell::new(HashMap::from_iter(iter))))
    }
}

impl Database for HashMapDb {
    type WriteBatch = HashMapWriteBatch;

    fn get_opt<K, V>(&self, key: &Key<K, V>) -> Result<Option<V>>
    where
        K: AsRef<[u8]>,
        V: serde::de::DeserializeOwned,
    {
        let k = key.as_ref().to_vec();
        let res = match RefCell::borrow(&self.rc).get(&k) {
            Some(value) => Some(bincode::deserialize(value.as_ref())?),
            None => None,
        };

        Ok(res)
    }

    fn contains<K, V>(&self, key: &Key<K, V>) -> Result<bool>
    where
        K: AsRef<[u8]>,
    {
        let k = key.as_ref().to_vec();
        let res = RefCell::borrow(&self.rc).contains_key(&k);

        Ok(res)
    }

    fn put<K, V, Vref>(&self, key: &Key<K, V>, value: Vref) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize + ?Sized,
        Vref: Borrow<V>,
    {
        let k = key.as_ref().to_vec();
        let v = bincode::serialize(value.borrow())?;

        self.rc.borrow_mut().insert(k, v);

        Ok(())
    }

    fn write(&self, batch: Self::WriteBatch) -> Result<()> {
        let mut map = self.rc.borrow_mut();

        for (k, v) in batch {
            map.insert(k, v);
        }

        Ok(())
    }

    fn flush(&self) -> Result<()> {
        Ok(())
    }

    fn batch(&self) -> Self::WriteBatch {
        Default::default()
    }
}

#[derive(Default)]
pub struct HashMapWriteBatch {
    data: HashMap<Bytes, Bytes>,
}

impl WriteBatch for HashMapWriteBatch {
    fn put<K, V, Vref>(&mut self, key: &Key<K, V>, value: Vref) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize + ?Sized,
        Vref: Borrow<V>,
    {
        let k = key.as_ref().to_vec();
        let v = bincode::serialize(value.borrow())?;

        self.data.insert(k, v);

        Ok(())
    }
}

type IntoIter = std::collections::hash_map::IntoIter<Bytes, Bytes>;

impl IntoIterator for HashMapWriteBatch {
    type Item = (Bytes, Bytes);
    type IntoIter = IntoIter;

    fn into_iter(self) -> IntoIter {
        self.data.into_iter()
    }
}

#[test]
fn test_hashmap_db() {
    let db = HashMapDb::default();
    let key: Key<_, Vec<u8>> = Key::new(b"key");
    let value = b"value".to_vec();

    assert!(!db.contains(&key).unwrap());
    assert!(db.get_opt(&key).unwrap().is_none());

    db.put(&key, &value).unwrap();

    assert!(db.contains(&key).unwrap());
    assert_eq!(value, db.get(&key).unwrap());
}

#[test]
fn test_hashmap_writebatch() {
    let db = HashMapDb::default();
    let mut batch = db.batch();
    let key1: Key<_, Vec<u8>> = Key::new(b"key1");
    let key2: Key<_, Vec<u8>> = Key::new(b"key2");
    let value1 = b"value1".to_vec();
    let value2 = b"value2".to_vec();

    batch.put(&key1, &value1).unwrap();
    batch.put(&key2, &value2).unwrap();

    db.write(batch).unwrap();

    assert_eq!(value1, db.get(&key1).unwrap());
    assert_eq!(value2, db.get(&key2).unwrap());
}
