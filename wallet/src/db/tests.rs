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

    fn get_opt<K, V>(&self, key: &K) -> Result<Option<V>>
    where
        K: AsRef<[u8]> + ?Sized,
        V: serde::de::DeserializeOwned,
    {
        let k = key.as_ref().to_vec();
        let res = match self.rc.borrow().get(&k) {
            Some(value) => Some(bincode::deserialize(value)?),
            None => None,
        };

        Ok(res)
    }

    fn contains<K>(&self, key: &K) -> Result<bool>
    where
        K: AsRef<[u8]> + ?Sized,
    {
        let k = key.as_ref().to_vec();
        let res = self.rc.borrow().contains_key(&k);

        Ok(res)
    }

    fn put<K, V>(&self, key: K, value: V) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize,
    {
        let k = key.as_ref().to_vec();
        let v = bincode::serialize(&value)?;

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
    fn put<K, V>(&mut self, key: K, value: V) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize,
    {
        let k = key.as_ref().to_vec();
        let v = bincode::serialize(&value)?;

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
    let storage: Rc<RefCell<HashMap<Bytes, Bytes>>> = Default::default();
    let db = HashMapDb::new(storage);

    assert!(!db.contains(b"key").unwrap());
    assert!(db.get_opt::<_, Bytes>(b"key").unwrap().is_none());

    db.put(b"key", b"value".to_vec()).unwrap();

    assert!(db.contains(b"key").unwrap());
    assert_eq!(b"value".to_vec(), db.get::<_, Bytes>(b"key").unwrap());
}

#[test]
fn test_hashmap_writebatch() {
    let storage: Rc<RefCell<HashMap<Bytes, Bytes>>> = Default::default();
    let db = HashMapDb::new(storage);
    let mut batch = db.batch();

    batch.put(b"key1", b"value1".to_vec()).unwrap();
    batch.put(b"key2", b"value2".to_vec()).unwrap();

    db.write(batch).unwrap();

    assert_eq!(b"value1".to_vec(), db.get::<_, Bytes>(b"key1").unwrap());
    assert_eq!(b"value2".to_vec(), db.get::<_, Bytes>(b"key2").unwrap());
}
