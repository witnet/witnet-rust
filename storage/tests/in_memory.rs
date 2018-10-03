use std::collections::HashMap;
use witnet_storage::backends::in_memory::InMemoryStorage;
use witnet_storage::storage::Storage;

#[test]
fn storage_instantiation() {
    // Instantiate a new `InMemoryStorage` through the constructor of the `Storage` trait.
    let actual = *InMemoryStorage::new(String::new()).unwrap();

    // Recreate the expected final state.
    let expected = InMemoryStorage {
        memory: HashMap::new(),
    };

    // The storage instantiated through the constructor should equal the manually constructed one.
    assert_eq!(actual, expected);
}

#[test]
fn storage_crud_create() {
    // Instantiate a new `InMemoryStorage` through the constructor of the `Storage` trait.
    let mut storage = InMemoryStorage {
        memory: HashMap::new(),
    };
    // This `&[u8]` will be used as key for the `put` method.
    let foo_slice = b"foo";
    // This `Vec<u8>` will be used as value for the `put` method.
    let bar_vec = b"bar".to_vec();

    // Put the value into the storage.
    let return_value = storage.put(foo_slice, bar_vec.clone()).unwrap();

    // Recreate the expected final state.
    let mut expected_memory: HashMap<&[u8], Vec<u8>> = HashMap::new();
    expected_memory.insert(foo_slice, bar_vec);
    let expected_storage = InMemoryStorage {
        memory: expected_memory,
    };

    // The `put` method's return value should be an unit (`()`).
    assert_eq!(return_value, ());
    // The final state of the storage should equal the expected value.
    assert_eq!(storage, expected_storage);
}

#[test]
fn storage_crud_read() {
    // This `&[u8]` will be used as key for the `get` method.
    let foo_slice = b"foo";
    // This `Vec<u8>` will be used as value for the `get` method.
    let bar_vec = b"bar".to_vec();
    // Recreate an `InMemoryStorage` with data in it.
    let mut memory: HashMap<&[u8], Vec<u8>> = HashMap::new();
    memory.insert(foo_slice, bar_vec.clone());
    let storage = InMemoryStorage { memory };

    // Get value from storage.
    let value = storage.get(foo_slice).unwrap().unwrap();

    // Recreate the expected final state.
    let mut expected_memory: HashMap<&[u8], Vec<u8>> = HashMap::new();
    expected_memory.insert(foo_slice, bar_vec.clone());
    let expected_storage = InMemoryStorage {
        memory: expected_memory,
    };

    // The value returned by `get` should equal the value used when constructing the storage.
    assert_eq!(value, bar_vec);
    // The final state of the storage should equal the expected value. This is just to ensure that
    // the `get` method does not mutate the entries in the storage.
    assert_eq!(storage, expected_storage);
}

#[test]
fn storage_crud_update() {
    // This `&[u8]` will be used as key for the `update` method.
    let foo_slice = b"foo";
    // This `Vec<u8>` will be used as initial value for the `update` method.
    let bar_vec = b"bar".to_vec();
    // This `Vec<u8>` will be used as final value for the `update` method.
    let beer_vec = b"beer".to_vec();
    // Recreate an `InMemoryStorage` with the initial data in it.
    let mut memory: HashMap<&[u8], Vec<u8>> = HashMap::new();
    memory.insert(foo_slice, bar_vec.clone());
    let mut storage = InMemoryStorage { memory };

    // Update the value in the storage.
    let return_value = storage.put(foo_slice, beer_vec.clone()).unwrap();

    // Recreate the expected final state.
    let mut expected_memory: HashMap<&[u8], Vec<u8>> = HashMap::new();
    expected_memory.insert(foo_slice, beer_vec);
    let expected_storage = InMemoryStorage {
        memory: expected_memory,
    };

    // The `put` method's return value should be an unit (`()`).
    assert_eq!(return_value, ());
    // The final state of the storage should equal the expected value.
    assert_eq!(storage, expected_storage);
}

#[test]
fn storage_crud_delete() {
    // This `&[u8]` will be used as key for the `delete` method.
    let foo_slice = b"foo";
    // This `Vec<u8>` will be used as initial value for the specified key.
    let bar_vec = b"bar".to_vec();
    // Recreate an `InMemoryStorage` with data in it.
    let mut memory: HashMap<&[u8], Vec<u8>> = HashMap::new();
    memory.insert(foo_slice, bar_vec.clone());
    let mut storage = InMemoryStorage { memory };

    // Delete the entry from the storage.
    let return_value = storage.delete(foo_slice).unwrap();

    // Recreate the expected final state.
    let expected_storage = InMemoryStorage {
        memory: HashMap::new(),
    };

    // The `put` method's return value should be the unit (`()`).
    assert_eq!(return_value, ());
    // The final state of the storage should equal the expected value.
    assert_eq!(storage, expected_storage);
}

#[test]
fn storage_get_nonexistent() {
    // Recreate an `InMemoryStorage` with no data in it.
    let storage = InMemoryStorage {
        memory: HashMap::new(),
    };

    // Get nonexistent key from storage.
    let value = storage.get(b"faux").unwrap();

    // The value returned by `get` should equal `None`.
    assert_eq!(value, None);
}

#[test]
fn storage_delete_nonexistent() {
    // Recreate an `InMemoryStorage` with no data in it.
    let mut storage = InMemoryStorage {
        memory: HashMap::new(),
    };

    // Get nonexistent key from storage.
    let value = storage.delete(b"faux").unwrap();

    // The value returned by `delete` should be the unit (`()`).
    assert_eq!(value, ());
}
